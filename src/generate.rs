//! Pure generation: schema plus seeded stream in, dataset out.

use crate::model::{Dataset, FieldType, Record, Schema, Value};
use crate::rng::Rng;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

fn value_for(kind: &FieldType, rng: &mut Rng) -> Value {
    match kind {
        FieldType::Int { min, max } => Value::Int(rng.range_i64(*min, *max)),
        FieldType::Float { min, max } => Value::Float(rng.range_i64(*min, *max)),
        FieldType::Bool => Value::Bool(rng.boolean()),
        FieldType::Enum(values) => Value::Str(rng.pick(values).to_string()),
        FieldType::String { prefix, min_len, max_len } => {
            let length = rng.range_i64(*min_len as i64, *max_len as i64) as usize;
            let mut text = prefix.clone();
            for _ in 0..length {
                let index = rng.below(ALPHABET.len() as u64) as usize;
                text.push(ALPHABET[index] as char);
            }
            Value::Str(text)
        }
        FieldType::List { inner, min_len, max_len } => {
            let length = rng.range_i64(*min_len as i64, *max_len as i64) as usize;
            let mut values = Vec::with_capacity(length);
            for _ in 0..length {
                values.push(value_for(inner, rng));
            }
            Value::List(values)
        }
    }
}

fn low_value(kind: &FieldType) -> Value {
    match kind {
        FieldType::Int { min, .. } => Value::Int(*min),
        FieldType::Float { min, .. } => Value::Float(*min),
        FieldType::Bool => Value::Bool(false),
        FieldType::Enum(values) => Value::Str(values[0].clone()),
        FieldType::String { prefix, min_len, .. } => {
            let mut text = prefix.clone();
            for _ in 0..*min_len {
                text.push('a');
            }
            Value::Str(text)
        }
        FieldType::List { inner, min_len, .. } => {
            Value::List((0..*min_len).map(|_| low_value(inner)).collect())
        }
    }
}

fn high_value(kind: &FieldType) -> Value {
    match kind {
        FieldType::Int { max, .. } => Value::Int(*max),
        FieldType::Float { max, .. } => Value::Float(*max),
        FieldType::Bool => Value::Bool(true),
        FieldType::Enum(values) => Value::Str(values[values.len() - 1].clone()),
        FieldType::String { prefix, max_len, .. } => {
            let mut text = prefix.clone();
            for _ in 0..*max_len {
                text.push('z');
            }
            Value::Str(text)
        }
        FieldType::List { inner, max_len, .. } => {
            Value::List((0..*max_len).map(|_| high_value(inner)).collect())
        }
    }
}

/// Boundary-value records: all-low, all-high, then one field raised to its
/// high boundary at a time. This mode does not consume the seeded stream, so
/// it is independent of `seed` and `count`.
pub fn generate_edges(schema: &Schema) -> Dataset {
    let columns: Vec<String> = schema.fields.iter().map(|field| field.name.clone()).collect();
    let lows: Vec<Value> = schema.fields.iter().map(|field| low_value(&field.kind)).collect();
    let highs: Vec<Value> = schema.fields.iter().map(|field| high_value(&field.kind)).collect();
    let mut rows: Vec<Vec<(String, Value)>> = Vec::new();

    rows.push(columns.iter().cloned().zip(lows.iter().cloned()).collect());
    rows.push(columns.iter().cloned().zip(highs.iter().cloned()).collect());
    for index in 0..schema.fields.len() {
        let row: Vec<(String, Value)> = columns
            .iter()
            .cloned()
            .zip(lows.iter().enumerate().map(|(position, value)| {
                if position == index { highs[position].clone() } else { value.clone() }
            }))
            .collect();
        rows.push(row);
    }

    let records = rows.into_iter().map(|values| Record { values }).collect();
    Dataset { record: schema.record.clone(), columns, records }
}

/// Generate every record from one seeded stream. No clock, no OS randomness,
/// no hash iteration: (schema, seed) fully determines the returned bytes.
pub fn generate(schema: &Schema) -> Dataset {
    let mut rng = Rng::new(schema.seed);
    let columns: Vec<String> = schema.fields.iter().map(|field| field.name.clone()).collect();
    let mut records = Vec::with_capacity(schema.count);
    for _ in 0..schema.count {
        let mut values = Vec::with_capacity(columns.len());
        for field in &schema.fields {
            values.push((field.name.clone(), value_for(&field.kind, &mut rng)));
        }
        records.push(Record { values });
    }
    Dataset { record: schema.record.clone(), columns, records }
}
