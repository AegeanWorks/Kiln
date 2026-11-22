//! Pure JSON Schema export. One-shot transformation, no I/O, no state.

use crate::model::{FieldType, Schema};
use crate::report::quoted;

fn cents(value: i64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

fn render_type(kind: &FieldType) -> String {
    match kind {
        FieldType::Int { min, max } => format!(
            "{{\"type\":\"integer\",\"minimum\":{},\"maximum\":{}}}",
            min, max
        ),
        FieldType::Float { min, max } => format!(
            "{{\"type\":\"number\",\"minimum\":{},\"maximum\":{}}}",
            cents(*min),
            cents(*max)
        ),
        FieldType::Bool => "{\"type\":\"boolean\"}".to_string(),
        FieldType::Enum(values) => {
            let items: Vec<String> = values.iter().map(|value| quoted(value)).collect();
            format!("{{\"type\":\"string\",\"enum\":[{}]}}", items.join(","))
        }
        FieldType::String { prefix, min_len, max_len } => {
            let base = prefix.len();
            let description = if prefix.is_empty() {
                String::new()
            } else {
                format!(",\"description\":{}", quoted(&format!("prefix: {}", prefix)))
            };
            format!(
                "{{\"type\":\"string\"{},\"minLength\":{},\"maxLength\":{}}}",
                description,
                base + min_len,
                base + max_len
            )
        }
        FieldType::List { inner, min_len, max_len } => format!(
            "{{\"type\":\"array\",\"items\":{},\"minItems\":{},\"maxItems\":{}}}",
            render_type(inner),
            min_len,
            max_len
        ),
    }
}

/// Render a draft-2020-12 JSON Schema describing one generated record.
pub fn render(schema: &Schema) -> String {
    let properties: Vec<String> = schema
        .fields
        .iter()
        .map(|field| format!("{}: {}", quoted(&field.name), render_type(&field.kind)))
        .collect();
    let required: Vec<String> = schema.fields.iter().map(|field| quoted(&field.name)).collect();
    let mut out = String::from("{\n");
    out.push_str("  \"$schema\": \"https://json-schema.org/draft/2020-12/schema\",\n");
    out.push_str(&format!("  \"title\": {},\n", quoted(&schema.record)));
    out.push_str("  \"type\": \"object\",\n");
    out.push_str(&format!("  \"properties\": {{\n    {}\n  }},\n", properties.join(",\n    ")));
    out.push_str(&format!("  \"required\": [{}],\n", required.join(",")));
    out.push_str("  \"additionalProperties\": false\n");
    out.push_str("}\n");
    out
}
