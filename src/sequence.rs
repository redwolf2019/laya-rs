//! Inference sequences and row-major CPU input buffers (compatibility.md §3–4).
//! SPDX-License-Identifier: Apache-2.0
//!
//! Port of he-jev/laya c5d78730's build_sequence/collate_items; see NOTICE.md.
//! Uses normalized requests and Python-compatible text, explicit option markers,
//! right truncation/padding, and the bundle's task budgets and special tokens.
//! `build` is atomic: lost markers reject the request, never shrink its answer space.
//! Buffers have ONNX dtypes i64/bool; qtype has shape [B]. No ORT or inference runs here.

use std::{collections::TryReserveError, error::Error as StdError, fmt};

use serde::Deserialize;
use tokenizers::Tokenizer;

use crate::system_one::{Criteria, Request, RequestError, Usage};

/// Sequence failures; request/marker errors map to invalid_request at the HTTP boundary.
#[derive(Debug)]
pub enum Error {
    Request(RequestError),
    MarkerLost,
    InvalidConfiguration,
    Configuration(serde_json::Error),
    Tokenizer(tokenizers::Error),
    SizeOverflow,
    Allocation(TryReserveError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "invalid sequence request",
            Self::MarkerLost => "option marker lost during truncation",
            Self::InvalidConfiguration | Self::Configuration(_) => "invalid sequence configuration",
            Self::Tokenizer(_) => "sequence tokenization failed",
            Self::SizeOverflow => "sequence dimensions overflow",
            Self::Allocation(_) => "sequence allocation failed",
        })
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Configuration(e) => Some(e),
            Self::Tokenizer(e) => Some(e.as_ref()),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}

/// Five input buffers in request order. Token and marker shapes are [B,L] and [B,K].
#[derive(Debug)]
pub struct Batch {
    pub token_shape: [usize; 2],
    pub marker_shape: [usize; 2],
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub marker_pos: Vec<i64>,
    pub marker_mask: Vec<bool>,
    pub qtype: Vec<i64>,
    pub usage: Usage,
}

struct SpecialTokens {
    cls: i64,
    sep: i64,
    mask: i64,
    pad: i64,
    mask_text: String,
}

impl SpecialTokens {
    fn read(tokenizer: &Tokenizer, bytes: &[u8]) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Names {
            cls_token: String,
            sep_token: String,
            mask_token: String,
            pad_token: String,
        }
        let names: Names = serde_json::from_slice(bytes).map_err(Error::Configuration)?;
        let id = |text: &str| -> Result<i64, Error> {
            let id = tokenizer
                .token_to_id(text)
                .ok_or(Error::InvalidConfiguration)?;
            let encoding = tokenizer.encode(text, false).map_err(Error::Tokenizer)?;
            if text.is_empty() || encoding.get_ids() != [id] {
                return Err(Error::InvalidConfiguration);
            }
            Ok(i64::from(id))
        };
        Ok(Self {
            cls: id(&names.cls_token)?,
            sep: id(&names.sep_token)?,
            mask: id(&names.mask_token)?,
            pad: id(&names.pad_token)?,
            mask_text: names.mask_token,
        })
    }
}

/// Reuses the loaded tokenizer; constructed once from verified bundle metadata.
pub struct SequenceBuilder {
    tokenizer: Tokenizer,
    special: SpecialTokens,
    max_len: usize,
    head_max_len: usize,
}

impl SequenceBuilder {
    /// Disable implicit padding/truncation and resolve special IDs from tokenizer_config.json.
    ///
    /// # Errors
    /// Invalid budgets/metadata or tokenizer errors. Bundle identity and pinned budgets
    /// must be checked by the loader; small synthetic budgets are allowed for oracle tests.
    pub fn new(
        mut tokenizer: Tokenizer,
        tokenizer_config: &[u8],
        max_len: usize,
        head_max_len: usize,
    ) -> Result<Self, Error> {
        if max_len == 0 || i64::try_from(max_len).is_err() || i64::try_from(head_max_len).is_err() {
            return Err(Error::InvalidConfiguration);
        }
        tokenizer.with_truncation(None).map_err(Error::Tokenizer)?;
        tokenizer.with_padding(None);
        let special = SpecialTokens::read(&tokenizer, tokenizer_config)?;
        Ok(Self {
            tokenizer,
            special,
            max_len,
            head_max_len,
        })
    }

    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Build one row per normalized question; count each unpadded state separately.
    ///
    /// # Errors
    /// Invalid text/empty batch/answer space, lost markers, tokenization failure,
    /// or unrepresentable dimensions/allocation. Validate resource limits with
    /// Request::from_slice before calling; no partial batch escapes on failure.
    pub fn build(&self, request: &Request) -> Result<Batch, Error> {
        if request.questions.is_empty() {
            return Err(Error::Request(RequestError::QuestionCount));
        }
        let state = self.encode(&request.state_text().map_err(Error::Request)?)?;
        let mut rows = reserved(request.questions.len())?;
        for question in &request.questions {
            if question.criteria.len() < 2 {
                return Err(Error::Request(RequestError::TooFewOptions));
            }
            let (kind, qtype) = match question.criteria {
                Criteria::Choice(_) => ("choice", 0),
                Criteria::Score(_) => ("score", 1),
                Criteria::Noul { .. } => ("noul", 2),
            };
            let instructions = question.instructions_text().map_err(Error::Request)?;
            let header = self.encode(&format!("{kind} question: {instructions}"))?;
            let options = question
                .criteria
                .option_texts()
                .iter()
                .map(|text| {
                    let tokens = self.encode(&format!(" {text}"))?;
                    Ok(std::iter::once(self.special.mask)
                        .chain(tokens.into_iter().take(48))
                        .collect())
                })
                .collect::<Result<Vec<Vec<i64>>, Error>>()?;
            rows.push(self.row(header, options, &state, qtype)?);
        }
        collate(rows, self.special.pad)
    }

    fn encode(&self, text: &str) -> Result<Vec<i64>, Error> {
        let cleaned = text.replace(&self.special.mask_text, " ");
        let encoded = self
            .tokenizer
            .encode(cleaned, false)
            .map_err(Error::Tokenizer)?;
        Ok(encoded.get_ids().iter().map(|id| i64::from(*id)).collect())
    }

    fn truncate_head(&self, header: &mut Vec<i64>, options: &mut [Vec<i64>]) -> Result<(), Error> {
        let mut option_len = sum_lengths(options)?;
        if self.head_max_len.saturating_sub(option_len) < 16 {
            // For head < 16 the signed floor is negative; max(4, floor) is still 4.
            let per = (self.head_max_len.saturating_sub(16) / options.len()).max(4);
            for option in &mut *options {
                option.truncate(per);
            }
            option_len = sum_lengths(options)?;
        }
        header.truncate(self.head_max_len.saturating_sub(option_len).max(8));
        Ok(())
    }

    fn row(
        &self,
        mut header: Vec<i64>,
        mut options: Vec<Vec<i64>>,
        state: &[i64],
        qtype: i64,
    ) -> Result<Row, Error> {
        self.truncate_head(&mut header, &mut options)?;
        let option_len = sum_lengths(&options)?;
        let head_len = header
            .len()
            .checked_add(option_len)
            .and_then(|n| n.checked_add(3))
            .ok_or(Error::SizeOverflow)?;
        let room = self.max_len.saturating_sub(head_len).saturating_sub(1);
        let state = &state[..state.len().min(room)];
        let capacity = head_len
            .checked_add(state.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::SizeOverflow)?;
        let mut ids = reserved(capacity)?;
        ids.push(self.special.cls);
        ids.extend(header);
        ids.push(self.special.sep);
        let mut markers = reserved(options.len())?;
        for option in options {
            if ids.len() >= self.max_len {
                return Err(Error::MarkerLost);
            }
            markers.push(i64::try_from(ids.len()).map_err(|_| Error::SizeOverflow)?);
            ids.extend(option);
        }
        ids.push(self.special.sep);
        ids.extend_from_slice(state);
        ids.push(self.special.sep);
        ids.truncate(self.max_len);
        Ok(Row {
            ids,
            markers,
            qtype,
        })
    }
}

struct Row {
    ids: Vec<i64>,
    markers: Vec<i64>,
    qtype: i64,
}

fn sum_lengths(options: &[Vec<i64>]) -> Result<usize, Error> {
    options.iter().try_fold(0_usize, |n, v| {
        n.checked_add(v.len()).ok_or(Error::SizeOverflow)
    })
}

fn reserved<T>(len: usize) -> Result<Vec<T>, Error> {
    let mut values = Vec::new();
    values.try_reserve_exact(len).map_err(Error::Allocation)?;
    Ok(values)
}

fn padded<T: Clone>(rows: usize, cols: usize, value: T) -> Result<Vec<T>, Error> {
    let len = rows.checked_mul(cols).ok_or(Error::SizeOverflow)?;
    i64::try_from(len).map_err(|_| Error::SizeOverflow)?;
    let mut values = reserved(len)?;
    values.resize(len, value);
    Ok(values)
}

fn collate(rows: Vec<Row>, pad: i64) -> Result<Batch, Error> {
    let b = rows.len();
    let l = rows
        .iter()
        .map(|row| row.ids.len())
        .max()
        .ok_or(Error::Request(RequestError::QuestionCount))?;
    let k = rows
        .iter()
        .map(|row| row.markers.len())
        .max()
        .ok_or(Error::Request(RequestError::QuestionCount))?;
    let mut batch = Batch {
        token_shape: [b, l],
        marker_shape: [b, k],
        input_ids: padded(b, l, pad)?,
        attention_mask: padded(b, l, 0)?,
        marker_pos: padded(b, k, 0)?,
        marker_mask: padded(b, k, false)?,
        qtype: reserved(b)?,
        usage: Usage::new(0),
    };
    for (i, row) in rows.into_iter().enumerate() {
        // Both products and their full buffer allocations were checked above;
        // row lengths are bounded by l/k, so all these ranges fit.
        let tokens = i * l..i * l + row.ids.len();
        let markers = i * k..i * k + row.markers.len();
        batch.input_ids[tokens.clone()].copy_from_slice(&row.ids);
        batch.attention_mask[tokens].fill(1);
        batch.marker_pos[markers.clone()].copy_from_slice(&row.markers);
        batch.marker_mask[markers].fill(true);
        batch.qtype.push(row.qtype);
        batch.usage.input_tokens = batch
            .usage
            .input_tokens
            .checked_add(row.ids.len())
            .ok_or(Error::SizeOverflow)?;
    }
    Ok(batch)
}
