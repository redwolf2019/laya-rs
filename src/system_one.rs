//! System One request normalization and response DTOs (compatibility.md §2/4/7).
//!
//! `Request::from_slice` is the input boundary. It checks body bytes, then the
//! complete UTF-8 JSON document/Unicode/depth, then effective fields and counts.
//! Root object depth is 1; only arrays/objects increase depth. Unknown fields are
//! ignored only after validation. Duplicate keys use last value/first position.
//! Required state/instructions accept every JSON type (including explicit null),
//! but missing fields fail. See [`json`] for lossless storage and ordered access.
//!
//! Questions retain input order. Choice requires a string array (deduplicated in
//! first-occurrence order) or string/null-valued object. Score requires a string
//! array and keeps duplicate levels. Both require 2..=max_options after decoding.
//! Noul criteria may be omitted; otherwise it must be an object whose recognized
//! false/true values are strings. Missing/empty descriptions use official defaults,
//! always false before true. Other criteria types/nulls fail. Names may be empty.
//!
//! Defaults: 1 MiB body, 1–16 questions, 2–32 Choice/Score options, depth 64.
//! Limits are supplied by the caller from CLI configuration; token limits and
//! transport readiness/media-type/streaming checks belong to later stages.
//! Errors contain no submitted names, values, parser text, or internal paths.
//! Response values must come from validated postprocessing: these DTOs do not run
//! inference, calibrate, round, or manufacture probabilities. Response::new fixes
//! model="rl-agent" and output_tokens=0; input_tokens is the batch mask sum.

pub mod json;

use std::{collections::HashSet, fmt};

use serde::Serialize;
use serde_json::value::RawValue;

use json::Value;

/// Input resource limits; match the validated CLI settings.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_body_bytes: usize,
    pub max_questions: usize,
    pub max_options: usize,
    pub max_json_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: 1_048_576,
            max_questions: 16,
            max_options: 32,
            max_json_depth: 64,
        }
    }
}

/// Static field locations, never caller-supplied keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Root,
    State,
    Questions,
    Question,
    Type,
    Instructions,
    Criteria,
}

/// Deterministic input failures. Count errors are distinct from JSON syntax errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    PayloadTooLarge,
    JsonSyntax,
    JsonDepth,
    MissingField(Field),
    InvalidField(Field),
    UnsupportedQuestionType,
    QuestionCount,
    TooFewOptions,
    TooManyOptions,
}

impl From<serde_json::Error> for RequestError {
    fn from(_: serde_json::Error) -> Self {
        Self::JsonSyntax
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.envelope().error.message)
    }
}

impl std::error::Error for RequestError {}

impl RequestError {
    /// HTTP status for this input boundary (transport/lifecycle errors are separate).
    pub fn status(&self) -> u16 {
        match self {
            Self::PayloadTooLarge => 413,
            _ => 400,
        }
    }

    /// Frozen public envelope; contains only static text, even for malformed JSON.
    pub fn envelope(&self) -> ErrorEnvelope {
        let (code, message) = match self {
            Self::PayloadTooLarge => ("payload_too_large", "Request body too large"),
            _ => ("invalid_request", "Invalid request"),
        };
        ErrorEnvelope {
            error: ErrorBody { code, message },
        }
    }
}

/// Static HTTP error envelope for input rejection.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// Error code/message contain no original parser diagnostics or input text.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: &'static str,
}

/// Owned normalized request; state/instructions retain their lossless JSON.
#[derive(Debug)]
pub struct Request {
    pub state: Box<RawValue>,
    pub questions: Vec<Question>,
}

impl Request {
    /// Model input text: strings verbatim, otherwise Python `ensure_ascii=False`.
    /// Invalid Unicode returns `JsonSyntax`; parse with `from_slice` first to
    /// validate all fields (including overwritten values) and resource limits.
    pub fn state_text(&self) -> Result<String, RequestError> {
        json::render(&self.state, false)
    }

    /// Reject invalid syntax, Unicode, fields and resource/model boundaries.
    /// Does not log input or call a model. A future HTTP reader must also bound
    /// bytes while receiving, before buffering the full request here.
    pub fn from_slice(body: &[u8], limits: &Limits) -> Result<Self, RequestError> {
        if body.len() > limits.max_body_bytes {
            return Err(RequestError::PayloadTooLarge);
        }
        let text = std::str::from_utf8(body).map_err(|_| RequestError::JsonSyntax)?;
        let raw: &RawValue = serde_json::from_str(text)?;
        json::validate(raw, limits.max_json_depth)?;
        let fields = object(raw, Field::Root)?;
        let state = required(&fields, "state", Field::State)?.to_owned();
        let questions = object(
            required(&fields, "questions", Field::Questions)?,
            Field::Questions,
        )?;
        if questions.is_empty() || questions.len() > limits.max_questions {
            return Err(RequestError::QuestionCount);
        }
        let questions = questions
            .into_iter()
            .map(|(name, raw)| Question::parse(name, raw, limits))
            .collect::<Result<_, _>>()?;
        Ok(Self { state, questions })
    }
}

/// One named question; criteria determines the question type and index space.
#[derive(Debug)]
pub struct Question {
    pub name: String,
    pub instructions: Box<RawValue>,
    pub criteria: Criteria,
}

impl Question {
    /// Model instruction text: strings verbatim, otherwise Python `ensure_ascii=True`.
    /// Uses the same input validation contract as [`Request::state_text`].
    pub fn instructions_text(&self) -> Result<String, RequestError> {
        json::render(&self.instructions, true)
    }

    fn parse(name: String, raw: &RawValue, limits: &Limits) -> Result<Self, RequestError> {
        let fields = object(raw, Field::Question)?;
        let kind = string(required(&fields, "type", Field::Type)?, Field::Type)?;
        if !matches!(kind.as_str(), "choice" | "score" | "noul") {
            return Err(RequestError::UnsupportedQuestionType);
        }
        let instructions = required(&fields, "instructions", Field::Instructions)?.to_owned();
        let criteria = match kind.as_str() {
            "choice" => Criteria::Choice(choice(required(&fields, "criteria", Field::Criteria)?)?),
            "score" => Criteria::Score(strings(required(&fields, "criteria", Field::Criteria)?)?),
            _ => noul(field(&fields, "criteria"))?,
        };
        if criteria.len() < 2 {
            return Err(RequestError::TooFewOptions);
        }
        if criteria.len() > limits.max_options {
            return Err(RequestError::TooManyOptions);
        }
        Ok(Self {
            name,
            instructions,
            criteria,
        })
    }
}

/// Choice keys and descriptions in original order; null and empty are distinct.
#[derive(Debug, PartialEq, Eq)]
pub struct ChoiceOption {
    pub key: String,
    pub description: Option<String>,
}

/// Normalized answer spaces; Score preserves duplicate descriptions.
#[derive(Debug, PartialEq, Eq)]
pub enum Criteria {
    Choice(Vec<ChoiceOption>),
    Score(Vec<String>),
    Noul {
        false_description: String,
        true_description: String,
    },
}

impl Criteria {
    /// Number of model options, after Choice duplicate folding.
    pub fn len(&self) -> usize {
        match self {
            Self::Choice(v) => v.len(),
            Self::Score(v) => v.len(),
            Self::Noul { .. } => 2,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Official option texts, before tokenizer MASK cleaning and truncation.
    pub fn option_texts(&self) -> Vec<String> {
        match self {
            Self::Choice(options) => options
                .iter()
                .map(|option| match &option.description {
                    Some(description) if !description.is_empty() => {
                        format!("{}: {description}", option.key)
                    }
                    _ => option.key.clone(),
                })
                .collect(),
            Self::Score(levels) => levels
                .iter()
                .enumerate()
                .map(|(i, description)| format!("level {i}: {description}"))
                .collect(),
            Self::Noul {
                false_description,
                true_description,
            } => vec![
                format!("false: {false_description}"),
                format!("true: {true_description}"),
            ],
        }
    }
}

fn object(raw: &RawValue, field: Field) -> Result<Vec<(String, &RawValue)>, RequestError> {
    match json::view(raw)? {
        Value::Object(v) => Ok(v),
        _ => Err(RequestError::InvalidField(field)),
    }
}

fn field<'a>(fields: &[(String, &'a RawValue)], key: &str) -> Option<&'a RawValue> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| *value)
}

fn required<'a>(
    fields: &[(String, &'a RawValue)],
    key: &str,
    path: Field,
) -> Result<&'a RawValue, RequestError> {
    field(fields, key).ok_or(RequestError::MissingField(path))
}

fn string(raw: &RawValue, field: Field) -> Result<String, RequestError> {
    match json::view(raw)? {
        Value::String(v) => Ok(v),
        _ => Err(RequestError::InvalidField(field)),
    }
}

fn strings(raw: &RawValue) -> Result<Vec<String>, RequestError> {
    let Value::Array(values) = json::view(raw)? else {
        return Err(RequestError::InvalidField(Field::Criteria));
    };
    values
        .into_iter()
        .map(|v| string(v, Field::Criteria))
        .collect()
}

fn choice(raw: &RawValue) -> Result<Vec<ChoiceOption>, RequestError> {
    match json::view(raw)? {
        Value::Array(_) => {
            let mut seen = HashSet::new();
            Ok(strings(raw)?
                .into_iter()
                .filter(|key| seen.insert(key.clone()))
                .map(|key| ChoiceOption {
                    key,
                    description: None,
                })
                .collect())
        }
        Value::Object(entries) => entries
            .into_iter()
            .map(|(key, value)| {
                let description = match json::view(value)? {
                    Value::Null => None,
                    Value::String(v) => Some(v),
                    _ => return Err(RequestError::InvalidField(Field::Criteria)),
                };
                Ok(ChoiceOption { key, description })
            })
            .collect(),
        _ => Err(RequestError::InvalidField(Field::Criteria)),
    }
}

fn noul(raw: Option<&RawValue>) -> Result<Criteria, RequestError> {
    let fields = raw
        .map(|v| object(v, Field::Criteria))
        .transpose()?
        .unwrap_or_default();
    let description = |key, default: &str| -> Result<String, RequestError> {
        match field(&fields, key)
            .map(|v| string(v, Field::Criteria))
            .transpose()?
        {
            Some(v) if !v.is_empty() => Ok(v),
            _ => Ok(default.to_owned()),
        }
    };
    Ok(Criteria::Noul {
        false_description: description("false", "no, the statement does not hold")?,
        true_description: description("true", "yes, the statement holds")?,
    })
}

/// Full success envelope. Values are supplied by the future postprocessor, not inferred here.
#[derive(Debug, Serialize)]
pub struct Response {
    model: &'static str,
    #[serde(serialize_with = "serialize_entries")]
    pub answers: Vec<(String, Answer)>,
    pub usage: Usage,
}

impl Response {
    /// Supply one answer per question in request order and the sum of input masks.
    /// Callers must validate finite model results and apply official rounding first.
    pub fn new(answers: Vec<(String, Answer)>, input_tokens: usize) -> Self {
        Self {
            model: "rl-agent",
            answers,
            usage: Usage::new(input_tokens),
        }
    }
}

/// Input tokens include special tokens and each repeated state; padding is excluded.
#[derive(Debug, Serialize)]
pub struct Usage {
    pub input_tokens: usize,
    output_tokens: usize,
}

impl Usage {
    pub(crate) fn new(input_tokens: usize) -> Self {
        Self {
            input_tokens,
            output_tokens: 0,
        }
    }
}

/// Action head probability, without the main answer's four-place rounding.
#[derive(Debug, Serialize)]
pub struct RlAgent {
    pub act_probability: f64,
}

/// Complete official answer fields. Ordered entries serialize as JSON objects.
/// Choice uses original keys; Score uses zero-based string indices for both maps.
/// Noul contains neither confidence nor probabilities. Numerical validation and
/// consistency with the normalized question are the postprocessor's responsibility.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Choice {
        choice: String,
        #[serde(serialize_with = "serialize_entries")]
        probabilities: Vec<(String, f64)>,
        confidence: f64,
        rl_agent: RlAgent,
    },
    Score {
        score: f64,
        #[serde(serialize_with = "serialize_entries")]
        legend: Vec<(String, String)>,
        #[serde(serialize_with = "serialize_entries")]
        probabilities: Vec<(String, f64)>,
        confidence: f64,
        rl_agent: RlAgent,
    },
    Noul {
        noul: f64,
        rl_agent: RlAgent,
    },
}

fn serialize_entries<T: Serialize, S: serde::Serializer>(
    entries: &[(String, T)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_map(entries.iter().map(|(key, value)| (key, value)))
}
