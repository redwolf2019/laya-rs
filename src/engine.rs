//! Synchronous System One execution over already-loaded, verified CPU resources.
//!
//! The caller owns the tokenizer/calibration and an exclusive Session execution
//! slot for the entire call. No model reload, retry, queue or native initialization
//! occurs here. An async server must run this CPU work on its blocking boundary;
//! dropping an HTTP waiter does not release a still-running slot.
//! Requests must first pass Request::from_slice with the configured resource limits.
//! Only complete, validated answers escape; error Display/envelopes contain static
//! text. Sources are retained for internal diagnosis and must not enter responses.

use std::{error::Error as StdError, fmt};

use ort::{
    session::Session,
    value::{DynValue, Tensor},
};

use crate::{
    postprocess::{self, Calibration, Outputs},
    sequence::{self, SequenceBuilder},
    system_one::{ErrorBody, ErrorEnvelope, Request, Response},
};

/// Inference failures retain the original typed source, without public diagnostics.
#[derive(Debug)]
pub enum Error {
    Sequence(sequence::Error),
    Runtime(ort::Error),
    Output(postprocess::Error),
    MissingOutput,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.envelope().error.message)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Sequence(e) => Some(e),
            Self::Runtime(e) => Some(e),
            Self::Output(e) => Some(e),
            Self::MissingOutput => None,
        }
    }
}

impl Error {
    /// Preserve request status; lost markers are 400, runtime/output failures are 500.
    pub fn status(&self) -> u16 {
        match self {
            Self::Sequence(sequence::Error::Request(e)) => e.status(),
            Self::Sequence(sequence::Error::MarkerLost) => 400,
            _ => 500,
        }
    }

    /// Frozen HTTP-compatible error data, independent of any router implementation.
    pub fn envelope(&self) -> ErrorEnvelope {
        if let Self::Sequence(sequence::Error::Request(e)) = self {
            return e.envelope();
        }
        let (code, message) = if self.status() == 400 {
            ("invalid_request", "Invalid request")
        } else {
            ("inference_failed", "Inference failed")
        };
        ErrorEnvelope {
            error: ErrorBody { code, message },
        }
    }
}

/// Execute a normalized request using one reusable, exclusively borrowed CPU Session.
/// # Errors
/// Sequence/tokenizer failures, tensor creation/execution/extraction failures,
/// missing outputs, shape mismatches or invalid model numbers. No partial response
/// is returned and no retry occurs; errors do not consume or replace the resources.
pub fn system_one(
    request: &Request,
    sequence: &SequenceBuilder,
    session: &mut Session,
    calibration: &Calibration,
) -> Result<Response, Error> {
    let batch = sequence.build(request).map_err(Error::Sequence)?;
    let tokens = batch.usage.input_tokens;
    let make_inputs = || -> ort::Result<_> {
        Ok(ort::inputs! {
            "input_ids" => Tensor::from_array((batch.token_shape, batch.input_ids))?,
            "attention_mask" => Tensor::from_array((batch.token_shape, batch.attention_mask))?,
            "marker_pos" => Tensor::from_array((batch.marker_shape, batch.marker_pos))?,
            "marker_mask" => Tensor::from_array((batch.marker_shape, batch.marker_mask))?,
            "qtype" => Tensor::from_array(([batch.marker_shape[0]], batch.qtype))?,
        })
    };
    let inputs = make_inputs().map_err(Error::Runtime)?;
    let outputs = tracing::info_span!("laya_session_run")
        .in_scope(|| session.run(inputs))
        .map_err(Error::Runtime)?;
    decode_outputs(
        request,
        calibration,
        outputs.get("logits"),
        outputs.get("act_probs"),
        tokens,
    )
}

fn decode_outputs(
    request: &Request,
    calibration: &Calibration,
    logits: Option<&DynValue>,
    action: Option<&DynValue>,
    input_tokens: usize,
) -> Result<Response, Error> {
    let (logits_shape, logits) = logits
        .ok_or(Error::MissingOutput)?
        .try_extract_tensor::<f32>()
        .map_err(Error::Runtime)?;
    let (act_probs_shape, act_probs) = action
        .ok_or(Error::MissingOutput)?
        .try_extract_tensor::<f32>()
        .map_err(Error::Runtime)?;
    calibration
        .response(
            request,
            Outputs {
                logits_shape,
                logits,
                act_probs_shape,
                act_probs,
            },
            input_tokens,
        )
        .map_err(Error::Output)
}
