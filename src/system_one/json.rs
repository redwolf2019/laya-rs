//! Lossless JSON storage and ordered views for compatibility.md §2.1.
//!
//! State/instructions remain `RawValue`s: integer/float lexemes, arbitrarily large
//! integers, negative zero and overflow/underflow exponents survive decoding.
//! [`view`] collapses object duplicates last-value/first-position at each level;
//! traverse child values with the same function. Rendering follows CPython 3.11.16
//! (compatibility.md §2.3): strings verbatim at the root, ordered containers with
//! comma/colon spaces, arbitrary integers, binary64 floats and Python escapes.
//! Request state keeps Unicode; question instructions escape non-ASCII as UTF-16.
//! Float digits use the locked serde_json/zmij shortest, ties-to-even conversion;
//! notation is explicitly adapted to Python, including signed two-digit exponents.
//! Validation visits the entire document before normalization, including ignored
//! fields and overwritten values. It uses an explicit stack, not Rust recursion.

use std::{collections::HashMap, fmt};

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::value::RawValue;

use super::RequestError;

/// A shallow view; container children borrow their exact original JSON.
#[derive(Debug)]
pub enum Value<'a> {
    Null,
    Bool(bool),
    /// Valid JSON integer lexeme, with no fixed precision limit.
    Integer(&'a str),
    /// Valid JSON float lexeme (decimal point or exponent), not yet binary64.
    Float(&'a str),
    String(String),
    Array(Vec<&'a RawValue>),
    Object(Vec<(String, &'a RawValue)>),
}

/// Decode one level, retaining object insertion order and last duplicate values.
/// Invalid Unicode at this level returns a static, typed error. Use the request
/// boundary to validate all descendants before consuming HTTP input here.
pub fn view(raw: &RawValue) -> Result<Value<'_>, RequestError> {
    let mut value = uncollapsed(raw)?;
    if let Value::Object(entries) = &mut value {
        let mut positions = HashMap::new();
        let mut unique: Vec<(String, &RawValue)> = Vec::new();
        for (key, value) in entries.drain(..) {
            let next = unique.len();
            let position = *positions.entry(key.clone()).or_insert(next);
            if position == next {
                unique.push((key, value));
            } else {
                unique[position].1 = value;
            }
        }
        *entries = unique;
    }
    Ok(value)
}

fn uncollapsed(raw: &RawValue) -> Result<Value<'_>, RequestError> {
    let text = raw.get();
    let value = match text.as_bytes().first() {
        Some(b'n') => Value::Null,
        Some(b't' | b'f') => Value::Bool(text == "true"),
        Some(b'"') => Value::String(serde_json::from_str(text)?),
        Some(b'[') => Value::Array(serde_json::from_str(text)?),
        Some(b'{') => Value::Object(serde_json::from_str::<Entries<'_>>(text)?.0),
        _ if text.contains(['.', 'e', 'E']) => Value::Float(text),
        _ => Value::Integer(text),
    };
    Ok(value)
}

struct Entries<'a>(Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Entries<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntriesVisitor;
        impl<'de> Visitor<'de> for EntriesVisitor {
            type Value = Entries<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(EntriesVisitor)
    }
}

pub(super) fn validate(raw: &RawValue, max_depth: usize) -> Result<(), RequestError> {
    let mut pending = vec![(raw, 0)];
    // ponytail: shallow RawValue parsing rescans ancestors (O(bytes * depth));
    // replace with a streaming visitor if larger configured depths need throughput.
    while let Some((raw, parent_depth)) = pending.pop() {
        let value = uncollapsed(raw)?;
        if matches!(value, Value::Array(_) | Value::Object(_)) && parent_depth >= max_depth {
            return Err(RequestError::JsonDepth);
        }
        match value {
            Value::Array(values) => {
                pending.extend(values.into_iter().map(|v| (v, parent_depth + 1)))
            }
            Value::Object(entries) => {
                pending.extend(entries.into_iter().map(|(_, v)| (v, parent_depth + 1)))
            }
            _ => {}
        }
    }
    Ok(())
}

enum Pending<'a> {
    Value(&'a RawValue),
    Key(String),
    Text(&'static str),
}

pub(super) fn render(raw: &RawValue, ascii: bool) -> Result<String, RequestError> {
    if let Value::String(text) = view(raw)? {
        return Ok(text);
    }
    let mut output = String::new();
    // Like validation, rendering uses a heap stack even for raised depth limits.
    let mut pending = vec![Pending::Value(raw)];
    while let Some(item) = pending.pop() {
        match item {
            Pending::Text(text) => output.push_str(text),
            Pending::Key(key) => quote(&key, ascii, &mut output),
            Pending::Value(raw) => append(view(raw)?, ascii, &mut output, &mut pending)?,
        }
    }
    Ok(output)
}

fn append<'a>(
    value: Value<'a>,
    ascii: bool,
    output: &mut String,
    pending: &mut Vec<Pending<'a>>,
) -> Result<(), RequestError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if value { "true" } else { "false" }),
        Value::Integer(text) => output.push_str(if text == "-0" { "0" } else { text }),
        Value::Float(text) => output.push_str(&float_text(text)?),
        Value::String(text) => quote(&text, ascii, output),
        Value::Array(values) => {
            output.push('[');
            pending.push(Pending::Text("]"));
            for (index, raw) in values.into_iter().enumerate().rev() {
                pending.push(Pending::Value(raw));
                if index != 0 {
                    pending.push(Pending::Text(", "));
                }
            }
        }
        Value::Object(entries) => {
            output.push('{');
            pending.push(Pending::Text("}"));
            for (index, (key, raw)) in entries.into_iter().enumerate().rev() {
                pending.push(Pending::Value(raw));
                pending.push(Pending::Text(": "));
                pending.push(Pending::Key(key));
                if index != 0 {
                    pending.push(Pending::Text(", "));
                }
            }
        }
    }
    Ok(())
}

fn quote(text: &str, ascii: bool, output: &mut String) {
    output.push('"');
    for ch in text.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch < ' ' || (ascii && ch >= '\u{7f}') => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    output.push_str(&format!("\\u{unit:04x}"));
                }
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
}

fn float_text(text: &str) -> Result<String, RequestError> {
    let value: f64 = text.parse().map_err(|_| RequestError::JsonSyntax)?;
    if value.is_infinite() {
        return Ok(if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        }
        .to_owned());
    }
    if value == 0.0 {
        return Ok(if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned());
    }
    // Only borrow the shortest decimal digits, never serde_json's dump notation.
    // Rust's default float display differs on halfway shortest representations.
    let shortest = serde_json::to_string(&value.abs())?;
    let (mantissa, exponent) = shortest.split_once('e').unwrap_or((&shortest, "0"));
    let exponent: i32 = exponent.parse().map_err(|_| RequestError::JsonSyntax)?;
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let digits = mantissa.replace('.', "");
    let leading = digits.len() - digits.trim_start_matches('0').len();
    let exponent = exponent + point - leading as i32 - 1;
    let digits = digits[leading..].trim_end_matches('0');
    let mut output = if value.is_sign_negative() { "-" } else { "" }.to_owned();
    append_decimal(digits, exponent, &mut output);
    Ok(output)
}

fn append_decimal(digits: &str, exponent: i32, output: &mut String) {
    if !(-4..16).contains(&exponent) {
        output.push_str(&digits[..1]);
        if digits.len() > 1 {
            output.push('.');
            output.push_str(&digits[1..]);
        }
        output.push_str(&format!("e{exponent:+03}"));
    } else if exponent < 0 {
        output.push_str("0.");
        output.push_str(&"0".repeat((-exponent - 1) as usize));
        output.push_str(digits);
    } else {
        let point = exponent as usize + 1;
        if point < digits.len() {
            output.push_str(&digits[..point]);
            output.push('.');
            output.push_str(&digits[point..]);
        } else {
            output.push_str(digits);
            output.push_str(&"0".repeat(point - digits.len()));
            output.push_str(".0");
        }
    }
}
