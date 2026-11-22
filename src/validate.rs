//! Pure fixture validation against a schema. No regeneration, no I/O.

use crate::json::Json;
use crate::model::{FieldType, Format, Schema, Verdict};
use crate::report::quoted;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

fn add(out: &mut Vec<Issue>, path: &str, message: String) {
    out.push(Issue { path: path.to_string(), message });
}

fn parse_cents(text: &str) -> Option<i64> {
    let (whole, fraction) = match text.split_once('.') {
        Some((left, right)) => (left, right),
        None => (text, ""),
    };
    if fraction.len() > 2 || fraction.is_empty() && !text.contains('.') && whole.is_empty() {
        return None;
    }
    let whole_value = whole.parse::<i64>().ok()?;
    let mut digits = fraction.to_string();
    while digits.len() < 2 {
        digits.push('0');
    }
    let fraction_value = digits.parse::<i64>().ok()?;
    Some(whole_value * 100 + fraction_value)
}

fn check_scalar(kind: &FieldType, value: &str, path: &str, out: &mut Vec<Issue>) {
    match kind {
        FieldType::Int { min, max } => match value.parse::<i64>() {
            Ok(number) if number >= *min && number <= *max => {}
            Ok(number) => add(out, path, format!("integer {} outside {}..{}", number, min, max)),
            Err(_) => add(out, path, format!("'{}' is not an integer", value)),
        },
        FieldType::Float { min, max } => match parse_cents(value) {
            Some(cents) if cents >= *min && cents <= *max => {}
            Some(cents) => add(out, path, format!("decimal {} outside {}..{}", cents / 100, min / 100, max / 100)),
            None => add(out, path, format!("'{}' is not a two-decimal number", value)),
        },
        FieldType::Bool => {
            if value != "true" && value != "false" {
                add(out, path, format!("'{}' is not a boolean", value));
            }
        }
        FieldType::Enum(values) => {
            if !values.iter().any(|item| item == value) {
                add(out, path, format!("'{}' is not an enum variant", value));
            }
        }
        FieldType::String { prefix, min_len, max_len } => {
            if !value.starts_with(prefix.as_str()) {
                add(out, path, format!("string does not start with '{}'", prefix));
            }
            let length = value.len();
            let low = prefix.len() + min_len;
            let high = prefix.len() + max_len;
            if length < low || length > high {
                add(out, path, format!("string length {} outside {}..{}", length, low, high));
            }
        }
        FieldType::List { .. } => add(out, path, "list value needs an array or pipe-separated cells".to_string()),
    }
}

fn check_json_value(kind: &FieldType, value: &Json, path: &str, out: &mut Vec<Issue>) {
    match (kind, value) {
        (FieldType::Int { .. } | FieldType::Float { .. }, Json::Num(raw)) => {
            check_scalar(kind, raw, path, out)
        }
        (FieldType::Bool, Json::Bool(_)) => {}
        (FieldType::Enum(_) | FieldType::String { .. }, Json::Str(text)) => {
            check_scalar(kind, text, path, out)
        }
        (FieldType::List { inner, min_len, max_len }, Json::Array(items)) => {
            if items.len() < *min_len || items.len() > *max_len {
                add(out, path, format!("list length {} outside {}..{}", items.len(), min_len, max_len));
            }
            for (index, item) in items.iter().enumerate() {
                check_json_value(inner, item, &format!("{}[{}]", path, index), out);
            }
        }
        _ => add(out, path, "value type does not match the schema".to_string()),
    }
}

fn validate_json(schema: &Schema, text: &str) -> Result<Vec<Issue>, String> {
    let document = crate::json::parse(text)?;
    let entries = match &document {
        Json::Object(entries) => entries,
        _ => return Err("fixture root must be a JSON object with a rows array".to_string()),
    };
    let rows = match entries.iter().find(|(key, _)| key == "rows") {
        Some((_, Json::Array(rows))) => rows,
        _ => return Err("fixture needs a rows array".to_string()),
    };
    let mut out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let fields = match row {
            Json::Object(fields) => fields,
            _ => {
                add(&mut out, &format!("rows[{}]", index), "row must be an object".to_string());
                continue;
            }
        };
        for field in &schema.fields {
            let path = format!("rows[{}].{}", index, field.name);
            match fields.iter().find(|(name, _)| name == &field.name) {
                Some((_, value)) => check_json_value(&field.kind, value, &path, &mut out),
                None => add(&mut out, &path, "missing field".to_string()),
            }
        }
        for (name, _) in fields {
            if !schema.fields.iter().any(|field| &field.name == name) {
                add(&mut out, &format!("rows[{}].{}", index, name), "unexpected field".to_string());
            }
        }
    }
    Ok(out)
}

fn validate_csv(schema: &Schema, text: &str) -> Result<Vec<Issue>, String> {
    let rows = crate::drift::parse_csv(text)?;
    if rows.is_empty() {
        return Err("CSV fixture is empty".to_string());
    }
    let mut out = Vec::new();
    let header = &rows[0];
    for (index, name) in header.iter().enumerate() {
        if schema.fields.get(index).map(|field| &field.name) != Some(name) {
            add(&mut out, &format!("header[{}]", index), format!("unexpected column '{}'", name));
        }
    }
    for (row_index, row) in rows.iter().enumerate().skip(1) {
        for (column, field) in schema.fields.iter().enumerate() {
            let path = format!("rows[{}].{}", row_index - 1, field.name);
            let cell = row.get(column).map(String::as_str).unwrap_or("");
            match &field.kind {
                FieldType::List { inner, min_len, max_len } => {
                    let items: Vec<&str> = if cell.is_empty() { Vec::new() } else { cell.split('|').collect() };
                    if items.len() < *min_len || items.len() > *max_len {
                        add(&mut out, &path, format!("list length {} outside {}..{}", items.len(), min_len, max_len));
                    }
                    for (index, item) in items.iter().enumerate() {
                        check_scalar(inner, item, &format!("{}[{}]", path, index), &mut out);
                    }
                }
                other => check_scalar(other, cell, &path, &mut out),
            }
        }
    }
    Ok(out)
}

fn validate_jsonl(schema: &Schema, text: &str) -> Result<Vec<Issue>, String> {
    let mut out = Vec::new();
    for (line_index, line) in text.lines().filter(|line| !line.trim().is_empty()).enumerate() {
        let document = crate::json::parse(line)?;
        let fields = match document {
            Json::Object(fields) => fields,
            _ => {
                add(&mut out, &format!("lines[{}]", line_index), "line must be a JSON object".to_string());
                continue;
            }
        };
        for field in &schema.fields {
            let path = format!("lines[{}].{}", line_index, field.name);
            match fields.iter().find(|(name, _)| name == &field.name) {
                Some((_, value)) => check_json_value(&field.kind, value, &path, &mut out),
                None => add(&mut out, &path, "missing field".to_string()),
            }
        }
    }
    Ok(out)
}

/// Validate a fixture of the given format. Parsing errors are returned so the
/// caller can diagnose them; schema violations are returned as issues.
pub fn validate(schema: &Schema, text: &str, format: Format) -> Result<Vec<Issue>, String> {
    match format {
        Format::Json => validate_json(schema, text),
        Format::Jsonl => validate_jsonl(schema, text),
        Format::Csv => validate_csv(schema, text),
    }
}

fn verdict(issues: &[Issue]) -> Verdict {
    if issues.is_empty() { Verdict::Pass } else { Verdict::Fail }
}

/// Human-readable report.
pub fn render_text(format: Format, issues: &[Issue]) -> String {
    let mut out = format!("fixture validation ({})", format.label());
    out.push_str(&format!(": {}\n", verdict(issues).label()));
    out.push_str(&format!("issues: {}\n", issues.len()));
    for issue in issues.iter().take(20) {
        out.push_str(&format!("  {}: {}\n", issue.path, issue.message));
    }
    if issues.len() > 20 {
        out.push_str(&format!("  ... and {} more\n", issues.len() - 20));
    }
    out
}

/// Machine-readable report with stable key order.
pub fn render_json(format: Format, issues: &[Issue]) -> String {
    let items: Vec<String> = issues.iter().map(|issue| {
        format!(
            "{{\"path\":{},\"message\":{}}}",
            quoted(&issue.path),
            quoted(&issue.message)
        )
    }).collect();
    format!(
        "{{\"schema\":1,\"command\":\"validate\",\"format\":{},\"verdict\":{},\"issues\":[{}]}}\n",
        quoted(format.label()),
        quoted(verdict(issues).label()),
        items.join(",")
    )
}
