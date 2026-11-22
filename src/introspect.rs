//! Pure schema introspection: one-shot, no state machine needed.

use crate::model::{FieldType, Schema};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: String,
    pub kind: &'static str,
    pub detail: String,
    pub bounds: String,
}

fn label(kind: &FieldType) -> &'static str {
    match kind {
        FieldType::Int { .. } => "int",
        FieldType::Float { .. } => "float",
        FieldType::Bool => "bool",
        FieldType::Enum(_) => "enum",
        FieldType::String { .. } => "string",
        FieldType::List { .. } => "list",
    }
}

fn detail(kind: &FieldType) -> String {
    match kind {
        FieldType::Enum(values) => format!("variants: {}", values.join(",")),
        FieldType::String { prefix, .. } => {
            if prefix.is_empty() { "no prefix".to_string() } else { format!("prefix: {}", prefix) }
        }
        FieldType::List { inner, .. } => format!("items: {}", label(inner)),
        _ => String::new(),
    }
}

fn bounds(kind: &FieldType) -> String {
    match kind {
        FieldType::Int { min, max } => format!("{}..{}", min, max),
        FieldType::Float { min, max } => format!(
            "{}.{:02}..{}.{:02}",
            min / 100,
            min % 100,
            max / 100,
            max % 100
        ),
        FieldType::String { min_len, max_len, .. } | FieldType::List { min_len, max_len, .. } => {
            format!("{}..{}", min_len, max_len)
        }
        FieldType::Bool => "false..true".to_string(),
        FieldType::Enum(values) => format!("{} variant(s)", values.len()),
    }
}

/// Describe fields in declaration order.
pub fn describe(schema: &Schema) -> Vec<FieldInfo> {
    schema.fields.iter().map(|field| FieldInfo {
        name: field.name.clone(),
        kind: label(&field.kind),
        detail: detail(&field.kind),
        bounds: bounds(&field.kind),
    }).collect()
}

/// Stable fixed-width table for a terminal.
pub fn render_text(schema: &Schema, fields: &[FieldInfo]) -> String {
    let mut out = format!(
        "schema: {}  format: {}  seed: {}  count: {}\n",
        schema.record,
        schema.format.label(),
        schema.seed,
        schema.count
    );
    let width = fields.iter().map(|field| field.name.len()).max().unwrap_or(5);
    out.push_str(&format!("{:<width$}  {:<7}  {:<24}  bounds\n", "field", "type", "detail", width = width));
    for field in fields {
        out.push_str(&format!(
            "{:<width$}  {:<7}  {:<24}  {}\n",
            field.name, field.kind, field.detail, field.bounds, width = width
        ));
    }
    out
}

/// Stable JSON array with the same information.
pub fn render_json(schema: &Schema, fields: &[FieldInfo]) -> String {
    let items: Vec<String> = fields.iter().map(|field| {
        format!(
            "{{\"name\":{},\"type\":{},\"detail\":{},\"bounds\":{}}}",
            crate::report::quoted(&field.name),
            crate::report::quoted(field.kind),
            crate::report::quoted(&field.detail),
            crate::report::quoted(&field.bounds)
        )
    }).collect();
    format!(
        "{{\"schema\":1,\"record\":{},\"fields\":[{}]}}\n",
        crate::report::quoted(&schema.record),
        items.join(",")
    )
}
