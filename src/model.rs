//! Frozen data types shared by the core. No behaviour beyond small labels.

/// Serialization target for a generated dataset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Csv,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Csv => "csv",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            other => Err(format!("unknown format '{}'; expected json or csv", other)),
        }
    }
}

/// A field's generator. Ranges are inclusive; floats are stored as integer
/// hundredths so generation never depends on floating-point accumulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    Int { min: i64, max: i64 },
    Float { min: i64, max: i64 },
    Bool,
    Enum(Vec<String>),
    String { prefix: String, min_len: usize, max_len: usize },
    List { inner: Box<FieldType>, min_len: usize, max_len: usize },
}

/// One declared field of the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: FieldType,
}

/// The parsed schema: the complete input to deterministic generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    pub version: u32,
    pub seed: u64,
    pub count: usize,
    pub format: Format,
    pub record: String,
    pub fields: Vec<Field>,
}

/// A generated value. Lists nest; everything else is scalar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Float(i64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
}

/// One generated record, preserving schema declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub values: Vec<(String, Value)>,
}

/// The generated dataset handed to the pure renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dataset {
    pub record: String,
    pub columns: Vec<String>,
    pub records: Vec<Record>,
}

/// Final output plus the metadata a gate needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub format: Format,
    pub text: String,
    pub records: usize,
    pub bytes: usize,
}

/// Outcome of a fixture comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
        }
    }

    pub fn from_equal(equal: bool) -> Self {
        if equal { Verdict::Pass } else { Verdict::Fail }
    }
}
