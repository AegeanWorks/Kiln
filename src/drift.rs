//! Pure, deterministic drift reports. When a fixture check fails, the
//! programmer gets paths and values instead of a bare "DIFFER".

use crate::json::Json;
use crate::model::Format;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
    pub expected: String,
    pub actual: String,
}

fn display(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(flag) => flag.to_string(),
        Json::Num(raw) => raw.clone(),
        Json::Str(text) => format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")),
        Json::Array(items) => {
            let parts: Vec<String> = items.iter().map(display).collect();
            format!("[{}]", parts.join(", "))
        }
        Json::Object(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|(key, value)| format!("{}: {}", display(&Json::Str(key.clone())), display(value)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn changed(path: String, expected: &Json, actual: &Json) -> Change {
    Change {
        path,
        kind: ChangeKind::Changed,
        expected: display(expected),
        actual: display(actual),
    }
}

fn diff_json(expected: &Json, actual: &Json, path: &str, out: &mut Vec<Change>) {
    match (expected, actual) {
        (Json::Object(left), Json::Object(right)) => {
            for (key, left_value) in left {
                let child = format!("{}.{}", path, key);
                match right.iter().find(|(other, _)| other == key) {
                    Some((_, right_value)) => diff_json(left_value, right_value, &child, out),
                    None => out.push(Change {
                        path: child,
                        kind: ChangeKind::Removed,
                        expected: display(left_value),
                        actual: String::new(),
                    }),
                }
            }
            for (key, right_value) in right {
                if !left.iter().any(|(other, _)| other == key) {
                    out.push(Change {
                        path: format!("{}.{}", path, key),
                        kind: ChangeKind::Added,
                        expected: String::new(),
                        actual: display(right_value),
                    });
                }
            }
        }
        (Json::Array(left), Json::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                let child = format!("{}[{}]", path, index);
                match (left.get(index), right.get(index)) {
                    (Some(a), Some(b)) => diff_json(a, b, &child, out),
                    (Some(a), None) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Removed,
                        expected: display(a),
                        actual: String::new(),
                    }),
                    (None, Some(b)) => out.push(Change {
                        path: child,
                        kind: ChangeKind::Added,
                        expected: String::new(),
                        actual: display(b),
                    }),
                    (None, None) => {}
                }
            }
        }
        (left, right) if left != right => out.push(changed(path.to_string(), left, right)),
        _ => {}
    }
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if quoted {
            if c == '"' {
                if index + 1 < chars.len() && chars[index + 1] == '"' {
                    cell.push('"');
                    index += 2;
                    continue;
                }
                quoted = false;
                index += 1;
                continue;
            }
            cell.push(c);
            index += 1;
            continue;
        }
        match c {
            '"' => {
                quoted = true;
                index += 1;
            }
            ',' => {
                row.push(cell.clone());
                cell.clear();
                index += 1;
            }
            '\n' => {
                row.push(cell.clone());
                cell.clear();
                rows.push(row.clone());
                row.clear();
                index += 1;
            }
            '\r' => index += 1,
            other => {
                cell.push(other);
                index += 1;
            }
        }
    }
    if quoted {
        return Err("unterminated quoted cell".to_string());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}

fn diff_csv(expected: &str, actual: &str) -> Result<Vec<Change>, String> {
    let left = parse_csv(expected)?;
    let right = parse_csv(actual)?;
    let header = left.first().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for index in 0..left.len().max(right.len()) {
        match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) => {
                for column in 0..a.len().max(b.len()) {
                    let name = header.get(column).cloned().unwrap_or_else(|| format!("col{}", column));
                    let path = format!("rows[{}].{}", index.saturating_sub(1), name);
                    match (a.get(column), b.get(column)) {
                        (Some(x), Some(y)) if x != y => out.push(Change {
                            path,
                            kind: ChangeKind::Changed,
                            expected: x.clone(),
                            actual: y.clone(),
                        }),
                        (Some(x), None) => out.push(Change {
                            path,
                            kind: ChangeKind::Removed,
                            expected: x.clone(),
                            actual: String::new(),
                        }),
                        (None, Some(y)) => out.push(Change {
                            path,
                            kind: ChangeKind::Added,
                            expected: String::new(),
                            actual: y.clone(),
                        }),
                        _ => {}
                    }
                }
            }
            (Some(a), None) => out.push(Change {
                path: format!("rows[{}]", index.saturating_sub(1)),
                kind: ChangeKind::Removed,
                expected: a.join(","),
                actual: String::new(),
            }),
            (None, Some(b)) => out.push(Change {
                path: format!("rows[{}]", index.saturating_sub(1)),
                kind: ChangeKind::Added,
                expected: String::new(),
                actual: b.join(","),
            }),
            (None, None) => {}
        }
    }
    Ok(out)
}

fn diff_jsonl(expected: &str, actual: &str) -> Result<Vec<Change>, String> {
    let left: Vec<&str> = expected.lines().filter(|line| !line.trim().is_empty()).collect();
    let right: Vec<&str> = actual.lines().filter(|line| !line.trim().is_empty()).collect();
    let mut out = Vec::new();
    for index in 0..left.len().max(right.len()) {
        let path = format!("lines[{}]", index);
        match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) => {
                let parsed_a = crate::json::parse(a)?;
                let parsed_b = crate::json::parse(b)?;
                diff_json(&parsed_a, &parsed_b, &path, &mut out);
            }
            (Some(a), None) => out.push(Change {
                path,
                kind: ChangeKind::Removed,
                expected: (*a).to_string(),
                actual: String::new(),
            }),
            (None, Some(b)) => out.push(Change {
                path,
                kind: ChangeKind::Added,
                expected: String::new(),
                actual: (*b).to_string(),
            }),
            (None, None) => {}
        }
    }
    Ok(out)
}

fn render_change(change: &Change) -> String {
    match change.kind {
        ChangeKind::Added => format!("  + {}: {}\n", change.path, change.actual),
        ChangeKind::Removed => format!("  - {}: {}\n", change.path, change.expected),
        ChangeKind::Changed => format!(
            "  ~ {}: expected {}, actual {}\n",
            change.path, change.expected, change.actual
        ),
    }
}

/// Structured changes between two fixture texts. The byte verdict from
/// `compare` remains authoritative; this is the explanation, not the gate.
pub fn changes(expected: &str, actual: &str, format: Format) -> Result<Vec<Change>, String> {
    match format {
        Format::Json => crate::json::parse(expected)
            .and_then(|left| crate::json::parse(actual).map(|right| (left, right)))
            .map(|(left, right)| {
                let mut found = Vec::new();
                diff_json(&left, &right, "$", &mut found);
                found
            }),
        Format::Jsonl => diff_jsonl(expected, actual),
        Format::Csv => diff_csv(expected, actual),
    }
}

pub fn count_changes(expected: &str, actual: &str, format: Format) -> usize {
    changes(expected, actual, format).map(|value| value.len()).unwrap_or(0)
}

/// A bounded, readable list of what changed. Parsing failures are reported as
/// such; the byte verdict from `compare` remains authoritative.
pub fn drift_report(expected: &str, actual: &str, format: Format) -> String {
    let changes = match changes(expected, actual, format) {
        Ok(value) => value,
        Err(error) => {
            return format!("fixture drift: cannot parse committed fixture as {}: {}\n", format.label(), error);
        }
    };
    if changes.is_empty() {
        return "fixture drift: bytes differ but no structural change was found\n".to_string();
    }
    let mut out = format!("fixture drift ({} change(s)):\n", changes.len());
    for change in changes.iter().take(12) {
        out.push_str(&render_change(change));
    }
    if changes.len() > 12 {
        out.push_str(&format!("  ... and {} more\n", changes.len() - 12));
    }
    out
}
