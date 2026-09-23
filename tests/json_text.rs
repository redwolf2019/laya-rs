//! Byte comparisons against the pinned CPython oracle, without a Python runtime.

use laya_server::system_one::{Limits, Request, RequestError};
use serde::Deserialize;

#[derive(Deserialize)]
struct Oracle {
    python: String,
    int_max_str_digits: usize,
    cases: Vec<Case>,
    rejected: Vec<Rejected>,
}

#[derive(Deserialize)]
struct Case {
    input_json: String,
    state_text: String,
    instructions_text: String,
}

#[derive(Deserialize)]
struct Rejected {
    input_json: String,
    python_error: String,
}

fn request(value: &str) -> Result<Request, RequestError> {
    let body = format!(
        r#"{{"state":{value},"questions":{{"q":{{"type":"noul","instructions":{value}}}}}}}"#
    );
    Request::from_slice(body.as_bytes(), &Limits::default())
}

#[test]
fn text_matches_cpython_bytes_in_both_modes() {
    let oracle: Oracle = serde_json::from_str(include_str!("fixtures/python-json.json")).unwrap();
    assert_eq!(oracle.python, "3.11.16");
    assert_eq!(oracle.int_max_str_digits, 0);
    for case in oracle.cases {
        let request = request(&case.input_json).unwrap();
        assert_eq!(
            request.state_text().unwrap().as_bytes(),
            case.state_text.as_bytes(),
            "state: {}",
            case.input_json
        );
        assert_eq!(
            request.questions[0].instructions_text().unwrap().as_bytes(),
            case.instructions_text.as_bytes(),
            "instructions: {}",
            case.input_json
        );
    }
    for case in oracle.rejected {
        let expected = match case.python_error.as_str() {
            "RecursionError" => RequestError::JsonDepth,
            "JSONDecodeError" => RequestError::JsonSyntax,
            error => panic!("unexpected oracle error {error}"),
        };
        assert_eq!(request(&case.input_json).unwrap_err(), expected);
    }
}
