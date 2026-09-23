//! Official temperature selection and typed answers (compatibility.md §5/9).
//! SPDX-License-Identifier: Apache-2.0
//!
//! Port of he-jev/laya c5d78730's system_one / confidence_from_probs / temp_bucket;
//! see NOTICE.md. Main arithmetic follows NumPy 2.3.3 float32; Score's weighted
//! sum promotes to float64. Decimal formatting rounds the exact binary64 value
//! to four places, ties to even, without a multiply that could erase midpoint sides.
//! Outputs are row-major float32: logits [B,K], already-softmaxed act_probs [B,2].
//! Validate every slot (including padding); only real options enter main softmax.
//! Fail the whole response on invalid outputs. No HTTP or native runtime is used.

use std::{collections::BTreeMap, fmt};

use serde::Deserialize;

use crate::system_one::{Answer, Criteria, Request, Response, RlAgent};

/// Static, deterministic failures; map output/numeric errors to inference_failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidTemperature,
    Shape,
    OutputIndex,
    NonFinite,
    Probability,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidTemperature => "model temperatures must be finite and positive",
            Self::Shape => "invalid model output shape",
            Self::OutputIndex => "model option index out of bounds",
            Self::NonFinite => "non-finite model output or calculation",
            Self::Probability => "invalid model probability distribution",
        })
    }
}

impl std::error::Error for Error {}

/// Validated bundle calibration. Missing fields use official defaults; null fails.
#[derive(Debug, Deserialize)]
#[serde(try_from = "Settings")]
pub struct Calibration {
    temperature: [f64; 3],
    temperature_by_options: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct Settings {
    #[serde(default = "default_temperature")]
    temperature: [f64; 3],
    #[serde(default)]
    temperature_by_options: BTreeMap<String, f64>,
}

fn default_temperature() -> [f64; 3] {
    [1.0; 3]
}

impl TryFrom<Settings> for Calibration {
    type Error = Error;

    fn try_from(settings: Settings) -> Result<Self, Error> {
        Self::new(settings.temperature, settings.temperature_by_options)
    }
}

/// Borrowed model buffers; the caller extracts tensors with dtype float32.
pub struct Outputs<'a> {
    pub logits_shape: &'a [i64],
    pub logits: &'a [f32],
    pub act_probs_shape: &'a [i64],
    pub act_probs: &'a [f32],
}

impl Calibration {
    /// Validate all temperatures, including unused buckets, before serving requests.
    /// # Errors
    /// InvalidTemperature for any non-finite or non-positive configured value.
    pub fn new(
        temperature: [f64; 3],
        temperature_by_options: BTreeMap<String, f64>,
    ) -> Result<Self, Error> {
        if temperature
            .iter()
            .chain(temperature_by_options.values())
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err(Error::InvalidTemperature);
        }
        Ok(Self {
            temperature,
            temperature_by_options,
        })
    }

    /// Bucket overrides take precedence over the corresponding type temperature.
    pub fn temperature(&self, criteria: &Criteria) -> f64 {
        let (kind, index) = match criteria {
            Criteria::Choice(_) => ("choice", 0),
            Criteria::Score(_) => ("score", 1),
            Criteria::Noul { .. } => ("noul", 2),
        };
        let bucket = match criteria.len() {
            0..=2 => "2",
            3..=5 => "3-5",
            6..=10 => "6-10",
            _ => "11+",
        };
        self.temperature_by_options
            .get(&format!("{kind}:{bucket}"))
            .copied()
            .unwrap_or(self.temperature[index])
    }

    /// Unrounded probabilities for the real k options; padding is checked but excluded.
    /// # Errors
    /// Missing options, invalid answer space, non-finite arithmetic or invalid probabilities.
    pub fn probabilities(&self, criteria: &Criteria, logits: &[f32]) -> Result<Vec<f32>, Error> {
        finite(logits)?;
        let k = criteria.len();
        let real = logits
            .get(..k)
            .filter(|_| k >= 2)
            .ok_or(Error::OutputIndex)?;
        let temperature = self.temperature(criteria) as f32;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(Error::NonFinite);
        }
        let mut p: Vec<_> = real.iter().map(|v| v / temperature).collect();
        finite(&p)?;
        let max = p.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        for value in &mut p {
            *value -= max;
        }
        finite(&p)?;
        for value in &mut p {
            *value = value.exp();
        }
        let sum: f32 = p.iter().sum();
        for value in &mut p {
            *value /= sum;
        }
        distribution(&p)?;
        Ok(p)
    }

    /// Construct every answer in request order, or return one deterministic error.
    /// input_tokens must be the Sequence Builder's unpadded mask sum.
    /// # Errors
    /// Shape/count/index mismatch, non-finite outputs or invalid probabilities.
    pub fn response(
        &self,
        request: &Request,
        outputs: Outputs<'_>,
        input_tokens: usize,
    ) -> Result<Response, Error> {
        let b = request.questions.len();
        let k = request
            .questions
            .iter()
            .map(|q| q.criteria.len())
            .max()
            .ok_or(Error::Shape)?;
        validate_shape(outputs.logits_shape, outputs.logits, b, k)?;
        validate_shape(outputs.act_probs_shape, outputs.act_probs, b, 2)?;
        let mut answers = Vec::with_capacity(b);
        for ((question, logits), act) in request
            .questions
            .iter()
            .zip(outputs.logits.chunks_exact(k))
            .zip(outputs.act_probs.as_chunks::<2>().0)
        {
            distribution(act)?;
            let p = self.probabilities(&question.criteria, logits)?;
            let answer = answer(&question.criteria, &p, f64::from(act[0]))?;
            answers.push((question.name.clone(), answer));
        }
        Ok(Response::new(answers, input_tokens))
    }
}

fn validate_shape(shape: &[i64], values: &[f32], b: usize, k: usize) -> Result<(), Error> {
    let rows = i64::try_from(b).map_err(|_| Error::Shape)?;
    let columns = i64::try_from(k).map_err(|_| Error::Shape)?;
    if b == 0 || k < 2 || shape != [rows, columns] || b.checked_mul(k) != Some(values.len()) {
        return Err(Error::Shape);
    }
    finite(values)
}

fn finite(values: &[f32]) -> Result<(), Error> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFinite);
    }
    Ok(())
}

fn distribution(p: &[f32]) -> Result<(), Error> {
    finite(p)?;
    // Frozen probability tolerance, reference sum=1: atol 1e-5 + rtol 1e-4.
    if p.iter().any(|v| !(0.0..=1.0).contains(v))
        || (p.iter().map(|v| f64::from(*v)).sum::<f64>() - 1.0).abs() > 0.00011
    {
        return Err(Error::Probability);
    }
    Ok(())
}

/// CPython round(float(value), 4), including exact midpoints and signed zero.
/// # Errors
/// Reject non-finite inputs or a failed decimal conversion.
pub fn round4(value: f64) -> Result<f64, Error> {
    if !value.is_finite() {
        return Err(Error::NonFinite);
    }
    format!("{value:.4}").parse().map_err(|_| Error::NonFinite)
}

fn confidence(p: &[f32]) -> Result<f64, Error> {
    let entropy = -p.iter().map(|v| v * v.clamp(1e-12, 1.0).ln()).sum::<f32>();
    round4(f64::from(1.0 - entropy / (p.len() as f64).ln() as f32))
}

fn answer(criteria: &Criteria, p: &[f32], act_probability: f64) -> Result<Answer, Error> {
    let rl_agent = RlAgent { act_probability };
    match criteria {
        Criteria::Choice(options) => {
            let mut best = 0;
            for i in 1..p.len() {
                if p[i] > p[best] {
                    best = i;
                }
            }
            Ok(Answer::Choice {
                choice: options[best].key.clone(),
                probabilities: rounded_probabilities(options.iter().map(|o| o.key.clone()), p)?,
                confidence: confidence(p)?,
                rl_agent,
            })
        }
        Criteria::Score(levels) => Ok(Answer::Score {
            score: round4(
                p.iter()
                    .enumerate()
                    .map(|(i, v)| i as f64 * f64::from(*v))
                    .sum(),
            )?,
            legend: levels
                .iter()
                .enumerate()
                .map(|(i, text)| (i.to_string(), text.clone()))
                .collect(),
            probabilities: rounded_probabilities((0..p.len()).map(|i| i.to_string()), p)?,
            confidence: confidence(p)?,
            rl_agent,
        }),
        Criteria::Noul { .. } => Ok(Answer::Noul {
            noul: round4(f64::from(p[1]))?,
            rl_agent,
        }),
    }
}

fn rounded_probabilities(
    keys: impl Iterator<Item = String>,
    p: &[f32],
) -> Result<Vec<(String, f64)>, Error> {
    keys.zip(p)
        .map(|(key, v)| Ok((key, round4(f64::from(*v))?)))
        .collect()
}
