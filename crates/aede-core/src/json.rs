//! Minimal JSON implementation (reading + writing), with no external
//! dependency.
//!
//! Aède persists its catalog in a JSON file whose structure mirrors the target
//! relational schema (`schema.sql`) exactly: one key per "table", each table
//! being an array of rows. Storage remains JSON; a future database migration
//! can preserve these concepts without changing the domain model.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A JSON value held in memory, as the catalog manipulates it between the file and the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// The `null` literal, which the catalog uses for an absent or unknown field.
    Null,
    /// The `true` and `false` literals.
    Bool(bool),
    /// A number: JSON knows only one numeric type, kept here as a double.
    Num(f64),
    /// A text string, already unescaped: the characters it denotes, not its written form.
    Str(String),
    /// An ordered sequence of values, the JSON array; a table of the schema is one of these.
    Arr(Vec<Json>),
    /// A set of key/value pairs, kept sorted by key so that two writes give the same bytes.
    Obj(BTreeMap<String, Json>),
}

impl Json {
    /// Opens an empty object, the starting point for building a row field by field.
    pub fn obj() -> Json {
        Json::Obj(BTreeMap::new())
    }

    /// Adds or replaces a field. The call is silently ignored when the value is not an object.
    pub fn set(&mut self, key: &str, value: Json) {
        if let Json::Obj(map) = self {
            map.insert(key.to_string(), value);
        }
    }

    /// Looks up a field by name; absent key and non-object alike give `None`, without failing.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(map) => map.get(key),
            _ => None,
        }
    }

    /// Borrows the text of a string value, with no copy; anything else gives `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Same reading as [`Json::as_str`], copied out for a caller that needs to keep the text.
    pub fn as_string(&self) -> Option<String> {
        self.as_str().map(|s| s.to_string())
    }

    /// Reads a number as stored, with no conversion or rounding.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// Reads an integral, finite count in the `u64` range. Fractional and
    /// out-of-range values are refused instead of being rounded or clamped.
    pub fn as_u64(&self) -> Option<u64> {
        self.as_f64().and_then(|n| {
            // u64::MAX rounds up to 2^64 as f64, so the upper bound is strict.
            if n.is_finite() && n >= 0.0 && n < u64::MAX as f64 && n.fract() == 0.0 {
                Some(n as u64)
            } else {
                None
            }
        })
    }

    /// Same reading as [`Json::as_u64`], narrowed for the small counters of the schema such as a
    /// year or a track number.
    pub fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|n| u32::try_from(n).ok())
    }

    /// Reads a boolean value; a `0` or a `"true"` string is not accepted as one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Borrows the elements of an array, the usual way to walk over the rows of a table.
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }

    /// Direct access to a typed field, tolerating a missing key.
    pub fn field_str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_string())
    }

    /// Reads a field as a count, absent or unusable giving `None`.
    pub fn field_u64(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_u64())
    }

    /// Reads a field as a small count, absent or unusable giving `None`.
    pub fn field_u32(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(|v| v.as_u32())
    }

    /// Reads a numeric field that may be fractional.
    pub fn field_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_f64())
    }

    /// Reads a boolean field, `None` when it is absent or of another type.
    ///
    /// Distinct from [`Json::field_bool`], which answers `false` for both: an
    /// imported measurement that was never taken is not a measurement that
    /// came out negative.
    pub fn field_optional_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    /// Reads a flag, where an absent field means `false`: an older catalog that predates the flag
    /// is read without special handling.
    pub fn field_bool(&self, key: &str) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }
}

impl From<&str> for Json {
    fn from(s: &str) -> Json {
        Json::Str(s.to_string())
    }
}
impl From<String> for Json {
    fn from(s: String) -> Json {
        Json::Str(s)
    }
}
impl From<u64> for Json {
    fn from(n: u64) -> Json {
        Json::Num(n as f64)
    }
}
impl From<u32> for Json {
    fn from(n: u32) -> Json {
        Json::Num(n as f64)
    }
}
impl From<usize> for Json {
    fn from(n: usize) -> Json {
        Json::Num(n as f64)
    }
}
impl From<i64> for Json {
    fn from(n: i64) -> Json {
        Json::Num(n as f64)
    }
}
impl From<f64> for Json {
    fn from(n: f64) -> Json {
        Json::Num(n)
    }
}
impl From<bool> for Json {
    fn from(b: bool) -> Json {
        Json::Bool(b)
    }
}
impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Json {
        match v {
            Some(x) => x.into(),
            None => Json::Null,
        }
    }
}
impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Json {
        Json::Arr(v.into_iter().map(Into::into).collect())
    }
}

// --------------------------------------------------------------------------
// Writing
// --------------------------------------------------------------------------

impl Json {
    /// Serializes to compact JSON.
    pub fn to_string_compact(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    /// Serializes to indented JSON, easier to inspect by hand.
    pub fn to_string_pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(2), 0);
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Num(n) => {
                if n.is_finite() {
                    if n.fract() == 0.0 && n.abs() < 9.0e15 {
                        let _ = write!(out, "{}", *n as i64);
                    } else {
                        let _ = write!(out, "{}", n);
                    }
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => write_escaped(out, s),
            Json::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline_indent(out, indent, depth + 1);
                    item.write(out, indent, depth + 1);
                }
                newline_indent(out, indent, depth);
                out.push(']');
            }
            Json::Obj(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline_indent(out, indent, depth + 1);
                    write_escaped(out, k);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    v.write(out, indent, depth + 1);
                }
                newline_indent(out, indent, depth);
                out.push('}');
            }
        }
    }
}

fn newline_indent(out: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(width) = indent {
        out.push('\n');
        for _ in 0..(width * depth) {
            out.push(' ');
        }
    }
}

fn write_escaped(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// --------------------------------------------------------------------------
// Reading
// --------------------------------------------------------------------------

/// Reason a document was refused, with enough detail to point at the guilty spot in the file.
#[derive(Debug)]
pub struct ParseError {
    /// What was expected at that point, phrased for someone opening the catalog in an editor.
    pub message: String,
    /// Position of the fault, counted in bytes from the start of the input.
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid JSON at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Reads a whole document and returns its root value.
///
/// The entire input must be consumed: anything after the root value is an error, so a truncated
/// or doubly written catalog is caught rather than half loaded.
/// Malformed Unicode, non-finite numbers and more than 128 nested containers
/// are rejected. Parse failures include a byte offset.
///
/// ```
/// let value = aede_core::json::parse(r#"{"count":3}"#)?;
/// assert_eq!(value.field_u32("count"), Some(3));
/// assert!(aede_core::json::parse("01").is_err());
/// # Ok::<(), aede_core::json::ParseError>(())
/// ```
pub fn parse(input: &str) -> Result<Json, ParseError> {
    let bytes = input.as_bytes();
    let mut p = Parser {
        bytes,
        pos: 0,
        depth: 0,
    };
    p.skip_ws();
    let value = p.value()?;
    p.skip_ws();
    if p.pos != bytes.len() {
        return Err(p.err("trailing data after the root value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            offset: self.pos,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", byte as char)))
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), ParseError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(self.err(&format!("expected '{word}'")))
        }
    }

    fn value(&mut self) -> Result<Json, ParseError> {
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'n') => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            Some(b't') => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[' | b'{') => {
                if self.depth >= 128 {
                    return Err(self.err("more than 128 nested containers"));
                }
                self.depth += 1;
                let result = if self.peek() == Some(b'[') {
                    self.array()
                } else {
                    self.object()
                };
                self.depth -= 1;
                result
            }
            Some(_) => self.number(),
        }
    }

    fn array(&mut self) -> Result<Json, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, ParseError> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let value = self.value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(map));
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or_else(|| self.err("unterminated string"))?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let esc = self.peek().ok_or_else(|| self.err("truncated escape"))?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let lo = self.hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err(self.err("invalid low UTF-16 surrogate"));
                                }
                                0x10000
                                    + ((u32::from(hi) - 0xD800) << 10)
                                    + (u32::from(lo) - 0xDC00)
                            } else {
                                u32::from(hi)
                            };
                            let ch = char::from_u32(cp)
                                .ok_or_else(|| self.err("invalid Unicode scalar"))?;
                            out.push(ch);
                        }
                        _ => return Err(self.err("unknown escape sequence")),
                    }
                }
                0x00..=0x1F => return Err(self.err("unescaped control character in string")),
                _ => {
                    // Advance over one complete UTF-8 character.
                    let start = self.pos;
                    let width = utf8_width(byte);
                    self.pos = (start + width).min(self.bytes.len());
                    match std::str::from_utf8(&self.bytes[start..self.pos]) {
                        Ok(s) => out.push_str(s),
                        Err(_) => return Err(self.err("invalid UTF-8 sequence")),
                    }
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u16, ParseError> {
        if self.pos + 4 > self.bytes.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let slice = &self.bytes[self.pos..self.pos + 4];
        let text =
            std::str::from_utf8(slice).map_err(|_| self.err("non-hexadecimal \\u escape"))?;
        let value =
            u16::from_str_radix(text, 16).map_err(|_| self.err("non-hexadecimal \\u escape"))?;
        self.pos += 4;
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.err("leading zero in number"));
                }
            }
            Some(b'1'..=b'9') => {
                self.digits()?;
            }
            _ => return Err(self.err("expected a number")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.digits()?;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.err("malformed number"))?;
        let number = text.parse::<f64>().map_err(|_| ParseError {
            message: "malformed number".into(),
            offset: start,
        })?;
        if !number.is_finite() {
            return Err(self.err("number is outside the finite range"));
        }
        Ok(Json::Num(number))
    }

    fn digits(&mut self) -> Result<(), ParseError> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.err("expected a decimal digit"));
        }
        Ok(())
    }
}

fn utf8_width(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
