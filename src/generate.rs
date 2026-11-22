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
