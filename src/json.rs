//! Small order-preserving JSON reader for Kiln's own rendered output.
//!
//! It is deliberately not a general-purpose JSON implementation: its job is to
//! turn a committed fixture into a comparable tree so drift can be reported by
//! path. Object key order is preserved, and numbers stay as their raw tokens.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct Reader {
    chars: Vec<char>,
    pos: usize,
}

impl Reader {
    fn new(text: &str) -> Self {
        Self { chars: text.chars().collect(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at offset {}", expected, self.pos))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let escaped = self.peek().ok_or_else(|| "unfinished escape".to_string())?;
                    self.pos += 1;
                    match escaped {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{0008}'),
                        'f' => out.push('\u{000C}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let mut value = 0u32;
                            for _ in 0..4 {
                                let hex = self.peek().ok_or_else(|| "short unicode escape".to_string())?;
                                self.pos += 1;
                                value = value * 16 + hex.to_digit(16).ok_or_else(|| "bad unicode escape".to_string())?;
                            }
                            let ch = char::from_u32(value).ok_or_else(|| "invalid unicode escape".to_string())?;
                            out.push(ch);
                        }
                        other => return Err(format!("unknown escape '{}'", other)),
                    }
                }
                other => out.push(other),
            }
        }
        Err("unterminated string".to_string())
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(format!("expected number at offset {}", start));
        }
        Ok(Json::Num(self.chars[start..self.pos].iter().collect()))
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        for expected in word.chars() {
            if self.peek() != Some(expected) {
                return Err(format!("expected '{}' at offset {}", word, self.pos));
            }
            self.pos += 1;
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some('"') => Ok(Json::Str(self.string()?)),
            Some('{') => {
                self.pos += 1;
                let mut entries = Vec::new();
                self.skip_ws();
                if self.peek() == Some('}') {
                    self.pos += 1;
                    return Ok(Json::Object(entries));
                }
                loop {
                    self.skip_ws();
                    let key = self.string()?;
                    self.skip_ws();
                    self.expect(':')?;
                    let value = self.value()?;
                    entries.push((key, value));
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some('}') => {
                            self.pos += 1;
                            return Ok(Json::Object(entries));
                        }
                        other => return Err(format!("expected ',' or '}}', found {:?}", other)),
                    }
                }
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == Some(']') {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(']') => {
                            self.pos += 1;
                            return Ok(Json::Array(items));
                        }
                        other => return Err(format!("expected ',' or ']', found {:?}", other)),
                    }
                }
            }
            Some('t') => self.literal("true", Json::Bool(true)),
            Some('f') => self.literal("false", Json::Bool(false)),
            Some('n') => self.literal("null", Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            other => Err(format!("unexpected token {:?} at offset {}", other, self.pos)),
        }
    }
}

/// Parse one JSON document, refusing trailing content.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut reader = Reader::new(text);
    let value = reader.value()?;
    reader.skip_ws();
    if reader.pos != reader.chars.len() {
        return Err(format!("trailing content at offset {}", reader.pos));
    }
    Ok(value)
}
