//! Lossless JSON storage and ordered views for compatibility.md §2.1.
//!
//! State/instructions remain `RawValue`s: integer/float lexemes, arbitrarily large
//! integers, negative zero and overflow/underflow exponents survive decoding.
//! [`view`] collapses object duplicates last-value/first-position at each level;
//! traverse child values with the same function. Number lexemes are NOT Python
//! rendered text: conversion/rounding and the two dumps modes belong to #11.
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
