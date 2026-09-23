//! Engine error envelopes are testable without initializing a native runtime.
use std::error::Error as _;

use laya_server::{engine::Error, postprocess, sequence, system_one::RequestError};

#[test]
fn errors_preserve_sources_and_expose_only_the_frozen_envelope() {
    for (error, status, code) in error_cases() {
        assert_eq!(error.status(), status);
        assert_eq!(error.envelope().error.code, code);
        assert!(error.source().is_some());
        let public = serde_json::to_string(&error.envelope()).unwrap();
        assert!(!public.contains("private"));
        assert!(!error.to_string().contains("private"));
    }
    let error = Error::Output(postprocess::Error::NonFinite);
    assert_eq!(
        error.source().unwrap().downcast_ref::<postprocess::Error>(),
        Some(&postprocess::Error::NonFinite)
    );
    assert_eq!(Error::MissingOutput.status(), 500);
    assert!(Error::MissingOutput.source().is_none());
}

fn error_cases() -> [(Error, u16, &'static str); 6] {
    [
        (
            Error::Sequence(sequence::Error::Request(RequestError::TooFewOptions)),
            400,
            "invalid_request",
        ),
        (
            Error::Sequence(sequence::Error::Request(RequestError::PayloadTooLarge)),
            413,
            "payload_too_large",
        ),
        (
            Error::Sequence(sequence::Error::MarkerLost),
            400,
            "invalid_request",
        ),
        (
            Error::Output(postprocess::Error::Shape),
            500,
            "inference_failed",
        ),
        (
            Error::Output(postprocess::Error::NonFinite),
            500,
            "inference_failed",
        ),
        (
            Error::Sequence(sequence::Error::Tokenizer(
                std::io::Error::other("private business text").into(),
            )),
            500,
            "inference_failed",
        ),
    ]
}
