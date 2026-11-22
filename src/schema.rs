//! Pure schema reader: text in, Schema out. No file handles, no writes.

use crate::model::{Field, FieldType, Format, Schema};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Num(i64),
    Str(String),
    Punct(char),
    DotDot,
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_rest(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lex_type(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '.' && i + 1 < chars.len() && chars[i + 1] == '.' {
            tokens.push(Token::DotDot);
            i += 2;
        } else if c == '(' || c == ')' || c == ',' {
            tokens.push(Token::Punct(c));
            i += 1;
        } else if c == '-' || c.is_ascii_digit() {
            let start = i;
            if c == '-' {
                i += 1;
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            let value = raw.parse::<i64>().map_err(|_| format!("bad number '{}'", raw))?;
            tokens.push(Token::Num(value));
        } else if c == '"' {
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '"' {
                    closed = true;
                    i += 1;
                    break;
                }
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    value.push(chars[i]);
                    i += 1;
                } else {
                    value.push(chars[i]);
                    i += 1;
                }
            }
            if !closed {
                return Err("unterminated string in field type".to_string());
            }
            tokens.push(Token::Str(value));
        } else if is_ident_start(c) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_rest(chars[i]) {
                i += 1;
            }
            tokens.push(Token::Ident(chars[start..i].iter().collect()));
        } else {
            return Err(format!("unexpected character '{}' in field type", c));
        }
    }
    Ok(tokens)
}

fn expect_punct(tokens: &[Token], pos: &mut usize, expected: char) -> Result<(), String> {
    match tokens.get(*pos) {
        Some(Token::Punct(c)) if *c == expected => {
            *pos += 1;
            Ok(())
        }
        other => Err(format!("expected '{}', found {:?}", expected, other)),
    }
}

fn expect_num(tokens: &[Token], pos: &mut usize) -> Result<i64, String> {
    match tokens.get(*pos) {
        Some(Token::Num(value)) => {
            *pos += 1;
            Ok(*value)
        }
        other => Err(format!("expected number, found {:?}", other)),
    }
}

fn expect_range(tokens: &[Token], pos: &mut usize) -> Result<(i64, i64), String> {
    let min = expect_num(tokens, pos)?;
    match tokens.get(*pos) {
        Some(Token::DotDot) => *pos += 1,
        other => return Err(format!("expected '..', found {:?}", other)),
    }
    let max = expect_num(tokens, pos)?;
    Ok((min, max))
}

fn parse_type(tokens: &[Token], pos: &mut usize) -> Result<FieldType, String> {
    let name = match tokens.get(*pos) {
        Some(Token::Ident(value)) => {
            *pos += 1;
            value.clone()
        }
        other => return Err(format!("expected a type name, found {:?}", other)),
    };
    match name.as_str() {
        "bool" => Ok(FieldType::Bool),
        "int" => {
            expect_punct(tokens, pos, '(')?;
            let (min, max) = expect_range(tokens, pos)?;
            expect_punct(tokens, pos, ')')?;
            Ok(FieldType::Int { min, max })
        }
        "float" => {
            expect_punct(tokens, pos, '(')?;
            let (min, max) = expect_range(tokens, pos)?;
            expect_punct(tokens, pos, ')')?;
            Ok(FieldType::Float { min, max })
        }
        "enum" => {
            expect_punct(tokens, pos, '(')?;
            let mut values = Vec::new();
            loop {
                match tokens.get(*pos) {
                    Some(Token::Ident(value)) => {
                        values.push(value.clone());
                        *pos += 1;
                    }
                    other => return Err(format!("expected enum variant, found {:?}", other)),
                }
                match tokens.get(*pos) {
                    Some(Token::Punct(',')) => *pos += 1,
                    Some(Token::Punct(')')) => {
                        *pos += 1;
                        break;
                    }
                    other => return Err(format!("expected ',' or ')', found {:?}", other)),
                }
            }
            Ok(FieldType::Enum(values))
        }
        "string" => {
            expect_punct(tokens, pos, '(')?;
            let prefix = match tokens.get(*pos) {
                Some(Token::Str(value)) => {
                    *pos += 1;
                    expect_punct(tokens, pos, ',')?;
                    value.clone()
                }
                _ => String::new(),
            };
            let (min_len, max_len) = expect_range(tokens, pos)?;
            expect_punct(tokens, pos, ')')?;
            Ok(FieldType::String { prefix, min_len: min_len as usize, max_len: max_len as usize })
        }
        "list" => {
            expect_punct(tokens, pos, '(')?;
            let inner = parse_type(tokens, pos)?;
            expect_punct(tokens, pos, ',')?;
            let (min_len, max_len) = expect_range(tokens, pos)?;
            expect_punct(tokens, pos, ')')?;
            Ok(FieldType::List {
                inner: Box::new(inner),
                min_len: min_len as usize,
                max_len: max_len as usize,
            })
        }
        other => Err(format!("unknown field type '{}'", other)),
    }
}

fn parse_field(line: &str) -> Result<Field, String> {
    let (name, type_text) = line
        .split_once(':')
        .ok_or_else(|| format!("field line needs 'name: type', got '{}'", line))?;
    let name = name.trim();
    if name.is_empty() || !is_ident_start(name.chars().next().unwrap_or(' '))
        || !name.chars().all(is_ident_rest)
    {
        return Err(format!("invalid field name '{}'", name));
    }
    let tokens = lex_type(type_text.trim())?;
    let mut pos = 0;
    let kind = parse_type(&tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(format!("trailing tokens in type of '{}'", name));
    }
    Ok(Field { name: name.to_string(), kind })
}

fn valid_record_name(name: &str) -> bool {
    !name.is_empty()
        && is_ident_start(name.chars().next().unwrap_or(' '))
        && name.chars().all(is_ident_rest)
}

/// Parse the line-oriented `.kiln` schema format.
pub fn parse(text: &str) -> Result<Schema, String> {
    let mut version = 1u32;
    let mut seed = 1u64;
    let mut count = 1usize;
    let mut format = Format::Json;
    let mut record = String::new();
    let mut fields = Vec::new();
    let mut in_record = false;

    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if in_record {
            if line == "}" {
                in_record = false;
                continue;
            }
            fields.push(parse_field(line).map_err(|error| format!("line {}: {}", number, error))?);
            continue;
        }
        if line.starts_with("record ") && line.ends_with('{') {
            let name = line[7..line.len() - 1].trim();
            if !valid_record_name(name) {
                return Err(format!("line {}: invalid record name '{}'", number, name));
            }
            record = name.to_string();
            in_record = true;
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected 'key = value' or 'record X {{'", number))?;
        let value = value.trim();
        match key.trim() {
            "version" => {
                version = value.parse().map_err(|_| format!("line {}: version must be an integer", number))?;
            }
            "seed" => {
                seed = value.parse().map_err(|_| format!("line {}: seed must be a u64", number))?;
            }
            "count" => {
                count = value.parse().map_err(|_| format!("line {}: count must be a positive integer", number))?;
            }
            "format" => {
                format = Format::parse(value).map_err(|error| format!("line {}: {}", number, error))?;
            }
            other => return Err(format!("line {}: unknown key '{}'", number, other)),
        }
    }
    if in_record {
        return Err("record block is missing its closing '}'".to_string());
    }
    Ok(Schema { version, seed, count, format, record, fields })
}

fn validate_type(kind: &FieldType, path: &str) -> Result<(), String> {
    match kind {
        FieldType::Int { min, max } | FieldType::Float { min, max } => {
            if min > max {
                return Err(format!("{}: range {}..{} is inverted", path, min, max));
            }
            if max - min > 1_000_000 {
                return Err(format!("{}: range is wider than 1000000", path));
            }
            if matches!(kind, FieldType::Float { .. }) && *min < 0 {
                return Err(format!("{}: float range must be non-negative", path));
            }
            Ok(())
        }
        FieldType::Bool => Ok(()),
        FieldType::Enum(values) => {
            if values.is_empty() || values.len() > 32 {
                return Err(format!("{}: enum needs 1 to 32 variants", path));
            }
            Ok(())
        }
        FieldType::String { prefix, min_len, max_len } => {
            if prefix.len() > 32 {
                return Err(format!("{}: prefix is longer than 32 bytes", path));
            }
            if min_len > max_len || *max_len > 64 {
                return Err(format!("{}: string length must be ordered and at most 64", path));
            }
            Ok(())
        }
        FieldType::List { inner, min_len, max_len } => {
            if min_len > max_len || *max_len > 16 {
                return Err(format!("{}: list length must be ordered and at most 16", path));
            }
            validate_type(inner, &format!("{}[]", path))
        }
    }
}

/// Validate bounds before any generation happens. Refuses oversized output.
pub fn validate(schema: &Schema) -> Result<(), String> {
    if schema.version != 1 {
        return Err(format!("unsupported schema version {}; this reader handles 1 only", schema.version));
    }
    if !(1..=10_000).contains(&schema.count) {
        return Err("count must be between 1 and 10000".to_string());
    }
    if !valid_record_name(&schema.record) {
        return Err("a 'record Name { ... }' block is required".to_string());
    }
    if schema.fields.is_empty() {
        return Err("record needs at least one field".to_string());
    }
    let mut seen = Vec::new();
    for field in &schema.fields {
        if seen.contains(&field.name) {
            return Err(format!("duplicate field '{}'", field.name));
        }
        seen.push(field.name.clone());
        validate_type(&field.kind, &field.name)?;
    }
    Ok(())
}
