//! Replay fixed official outputs; no model, tokenizer, Python or ORT is required.
use laya_server::{
    postprocess::{Calibration, Error, Outputs, round4},
    system_one::{Limits, Request},
};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(format!("tests/fixtures/system-one/{name}.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn artificial_buckets_fallback_ties_and_rounding_match_the_official_oracle() {
    let data = fixture("artificial");
    for (index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let request = Request::from_slice(
            &serde_json::to_vec(
                &serde_json::json!({"state": null, "questions": case["questions"]}),
            )
            .unwrap(),
            &Limits::default(),
        )
        .unwrap();
        let mut overrides = std::collections::BTreeMap::new();
        if case["source"] == "override" {
            overrides.insert(
                case["bucket"].as_str().unwrap().to_owned(),
                case["temperature"].as_f64().unwrap(),
            );
        } else {
            // An unrelated bucket must not suppress the type fallback.
            overrides.insert("unmatched:2".to_owned(), 42.0);
        }
        let calibration = Calibration::new([0.7, 1.3, 2.0], overrides).unwrap();
        assert_eq!(
            calibration.temperature(&request.questions[0].criteria),
            case["temperature"].as_f64().unwrap(),
            "artificial/{index}/temperature"
        );
        assert_json(
            &replay(&calibration, &request, case, 1),
            &case["response"],
            &format!("artificial/{index}"),
        );
    }
}

#[test]
fn artificial_ties_and_zero_probabilities_match_the_official_oracle() {
    let data = fixture("artificial");
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    for (index, case) in data["boundaries"].as_array().unwrap().iter().enumerate() {
        let request = Request::from_slice(
            &serde_json::to_vec(
                &serde_json::json!({"state": null, "questions": case["questions"]}),
            )
            .unwrap(),
            &Limits::default(),
        )
        .unwrap();
        assert_json(
            &replay(&calibration, &request, case, 1),
            &case["response"],
            &format!("boundary/{index}"),
        );
    }
}

#[test]
fn invalid_temperatures_and_json_config_fail_before_inference() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            Calibration::new([1.0, bad, 1.0], Default::default()).unwrap_err(),
            Error::InvalidTemperature
        );
        assert_eq!(
            Calibration::new([1.0; 3], [("unused".to_owned(), bad)].into()).unwrap_err(),
            Error::InvalidTemperature
        );
    }
    for json in [
        r#"{"temperature":null}"#,
        r#"{"temperature":[1,2]}"#,
        r#"{"temperature_by_options":null}"#,
        r#"{"temperature":[1,1e999,1]}"#,
    ] {
        assert!(serde_json::from_str::<Calibration>(json).is_err());
    }
}

fn mixed_request() -> Request {
    Request::from_slice(br#"{"state":null,"questions":{"a":{"type":"noul","instructions":null},"b":{"type":"score","instructions":null,"criteria":["a","b","c"]}}}"#, &Limits::default()).unwrap()
}

fn valid_output() -> Outputs<'static> {
    Outputs {
        logits_shape: &[2, 3],
        logits: &[0.0; 6],
        act_probs_shape: &[2, 2],
        act_probs: &[0.12345678, 0.876_543_2, 1.0, 0.0],
    }
}

fn run(output: Outputs<'_>) -> Result<laya_server::system_one::Response, Error> {
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    calibration.response(&mixed_request(), output, 4)
}

#[test]
fn output_shapes_and_missing_buffer_indices_fail() {
    for shape in [
        &[][..],
        &[2][..],
        &[2, 3, 1][..],
        &[1, 3][..],
        &[2, 2][..],
        &[-1, 3][..],
        &[i64::MAX, 3][..],
    ] {
        let mut output = valid_output();
        output.logits_shape = shape;
        assert_eq!(run(output).unwrap_err(), Error::Shape);
    }
    for shape in [&[2, 3][..], &[2][..], &[1, 2][..], &[2, 2, 1][..]] {
        let mut output = valid_output();
        output.act_probs_shape = shape;
        assert_eq!(run(output).unwrap_err(), Error::Shape);
    }
    let mut output = valid_output();
    output.logits = &output.logits[..5];
    assert_eq!(run(output).unwrap_err(), Error::Shape);
    let mut output = valid_output();
    output.act_probs = &output.act_probs[..3];
    assert_eq!(run(output).unwrap_err(), Error::Shape);
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    assert_eq!(
        calibration
            .probabilities(&mixed_request().questions[1].criteria, &[1.0, 2.0])
            .unwrap_err(),
        Error::OutputIndex
    );
}

#[test]
fn nonfinite_outputs_including_padding_fail_atomically() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for index in 0..6 {
            let mut logits = [0.0; 6];
            logits[index] = bad;
            let mut output = valid_output();
            output.logits = &logits;
            assert_eq!(run(output).unwrap_err(), Error::NonFinite);
        }
        let mut output = valid_output();
        let logits = [bad; 6];
        output.logits = &logits;
        assert_eq!(run(output).unwrap_err(), Error::NonFinite);
        for index in 0..4 {
            let mut act = [0.5; 4];
            act[index] = bad;
            let mut output = valid_output();
            output.act_probs = &act;
            assert_eq!(run(output).unwrap_err(), Error::NonFinite);
        }
    }
}

#[test]
fn action_is_validated_without_recalibration_or_rounding() {
    for bad in [
        [1.0, 0.0, 0.2, 0.7],
        [1.0, 0.0, -0.1, 1.1],
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 1.0, 1.0],
    ] {
        let mut output = valid_output();
        output.act_probs = &bad;
        assert_eq!(run(output).unwrap_err(), Error::Probability);
    }
    let expected = serde_json::to_value(run(valid_output()).unwrap()).unwrap();
    let mut padded = valid_output();
    padded.logits = &[0.0, 0.0, f32::MAX, 0.0, 0.0, 0.0];
    assert_json(
        &serde_json::to_value(run(padded).unwrap()).unwrap(),
        &expected,
        "padding",
    );
    assert_eq!(
        expected["answers"]["a"]["rl_agent"]["act_probability"].as_f64(),
        Some(f64::from(0.12345678_f32))
    );
    // Inside the frozen sum tolerance, preserve the original action value too.
    let mut output = valid_output();
    output.act_probs = &[0.12345678, 0.8766, 1.0, 0.0];
    let actual = serde_json::to_value(run(output).unwrap()).unwrap();
    assert_json(&actual, &expected, "action-tolerance");
}

#[test]
fn artificial_extreme_logits_are_stable_or_fail_on_nonfinite_intermediates() {
    let request = mixed_request();
    let criteria = &request.questions[0].criteria;
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    for logits in [[f32::MAX; 2], [f32::MIN; 2], [1e30; 2], [-1e30; 2]] {
        assert_eq!(
            calibration.probabilities(criteria, &logits).unwrap(),
            [0.5, 0.5]
        );
    }
    assert_eq!(
        calibration
            .probabilities(criteria, &[f32::MAX, f32::MIN])
            .unwrap_err(),
        Error::NonFinite
    );
    for temperature in [0.5, f64::MIN_POSITIVE, f64::MAX] {
        let calibration = Calibration::new([temperature; 3], Default::default()).unwrap();
        assert_eq!(
            calibration
                .probabilities(criteria, &[f32::MAX; 2])
                .unwrap_err(),
            Error::NonFinite
        );
    }
    let invalid = laya_server::system_one::Criteria::Score(vec![]);
    assert_eq!(
        calibration.probabilities(&invalid, &[]).unwrap_err(),
        Error::OutputIndex
    );
}

#[test]
fn cpython_decimal_rounding_preserves_midpoint_sides_and_signed_zero() {
    // CPython 3.11.16 development probe; explicitly artificial binary64 values.
    for (value, expected) in [
        (-0.0, -0.0_f64),
        (-0.00001, -0.0),
        (0.00005, 0.0001),
        (0.00015, 0.0001),
        (1.23445, 1.2345),
        (1.23455, 1.2346),
        (-0.03125, -0.0312),
    ] {
        assert_eq!(round4(value).unwrap().to_bits(), expected.to_bits());
    }
    for case in fixture("artificial")["rounding"].as_array().unwrap() {
        assert_eq!(
            round4(case["input"].as_f64().unwrap()).unwrap(),
            case["expected"].as_f64().unwrap()
        );
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(round4(bad).unwrap_err(), Error::NonFinite);
    }
}

fn replay(calibration: &Calibration, request: &Request, data: &Value, tokens: usize) -> Value {
    let logits: Vec<Vec<f32>> = serde_json::from_value(data["logits"].clone()).unwrap();
    let act: Vec<Vec<f32>> = serde_json::from_value(data["act_probs"].clone()).unwrap();
    let output = Outputs {
        logits_shape: &[logits.len() as i64, logits[0].len() as i64],
        logits: &logits.into_iter().flatten().collect::<Vec<_>>(),
        act_probs_shape: &[act.len() as i64, act[0].len() as i64],
        act_probs: &act.into_iter().flatten().collect::<Vec<_>>(),
    };
    serde_json::to_value(calibration.response(request, output, tokens).unwrap()).unwrap()
}

fn assert_json(actual: &Value, expected: &Value, location: &str) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{location}: field count");
            for (i, ((ak, av), (bk, bv))) in a.iter().zip(b).enumerate() {
                assert!(ak == bk, "{location}: key index {i}");
                assert_json(av, bv, &format!("{location}/field[{i}]"));
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64(), b.as_f64(), "{location}: numeric difference");
        }
        _ => assert!(
            actual == expected,
            "{location}: value mismatch (text redacted)"
        ),
    }
}

#[test]
fn all_official_and_onnx_logits_match_their_official_postprocessing_replays() {
    let manifest = fixture("manifest");
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap().trim_end_matches(".json");
        let data = fixture(name);
        if data["kind"] != "official_model" {
            continue;
        }
        let request = Request::from_slice(
            data["request_json"].as_str().unwrap().as_bytes(),
            &Limits::default(),
        )
        .unwrap();
        let tokens = data["response"]["usage"]["input_tokens"].as_u64().unwrap() as usize;
        for (head, values) in [("official", &data), ("onnx", &data["onnx"])] {
            let actual = replay(&calibration, &request, values, tokens);
            assert_json(&actual, &values["response"], &format!("{name}/{head}"));
        }
        assert_stages(&calibration, &request, &data, name);
    }
}

fn assert_stages(calibration: &Calibration, request: &Request, data: &Value, name: &str) {
    for (i, (question, row)) in request
        .questions
        .iter()
        .zip(data["rows"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            calibration.temperature(&question.criteria),
            row["temperature"]["value"].as_f64().unwrap()
        );
        let logits: Vec<f32> = serde_json::from_value(data["logits"][i].clone()).unwrap();
        let actual = calibration
            .probabilities(&question.criteria, &logits)
            .unwrap();
        for (j, (a, b)) in actual
            .iter()
            .zip(row["probabilities"].as_array().unwrap())
            .enumerate()
        {
            let b = b.as_f64().unwrap();
            let delta = (f64::from(*a) - b).abs();
            assert!(
                delta <= 1e-5 + 1e-4 * b.abs(),
                "{name}/row[{i}]/probabilities[{j}]: delta={delta}"
            );
        }
    }
}

#[test]
fn onnx_outputs_match_official_answers_exactly() {
    let calibration: Calibration = serde_json::from_str("{}").unwrap();
    let outputs = normalized_outputs();
    for file in fixture("manifest")["files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap().trim_end_matches(".json");
        let data = fixture(name);
        if data["kind"] != "official_model" {
            continue;
        }
        let request = Request::from_slice(
            data["request_json"].as_str().unwrap().as_bytes(),
            &Limits::default(),
        )
        .unwrap();
        let tokens = data["response"]["usage"]["input_tokens"].as_u64().unwrap() as usize;
        let mut actual = replay(
            &calibration,
            &request,
            &outputs["cases"][format!("{name}.json")],
            tokens,
        );
        let mut expected = data["response"].clone();
        for (i, (a, e)) in actual["answers"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .zip(expected["answers"].as_object_mut().unwrap().values_mut())
            .enumerate()
        {
            let a_act = a.as_object_mut().unwrap().remove("rl_agent").unwrap();
            let e_act = e.as_object_mut().unwrap().remove("rl_agent").unwrap();
            let a_act = a_act["act_probability"].as_f64().unwrap();
            let e_act = e_act["act_probability"].as_f64().unwrap();
            assert!(
                (a_act - e_act).abs() <= 1e-5 + 1e-4 * e_act.abs(),
                "{name}/answer[{i}]/act_probability"
            );
        }
        assert_json(&actual, &expected, name);
    }
}

fn normalized_outputs() -> Value {
    use sha2::{Digest, Sha256};
    let outputs: Value =
        serde_json::from_str(include_str!("fixtures/normalized-outputs.json")).unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("../docs/model-manifest.json")).unwrap();
    let graph = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "laya.onnx")
        .unwrap();
    assert_eq!(outputs["graph_sha256"], graph["sha256"]);
    let bytes = include_bytes!("fixtures/system-one/manifest.json");
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(outputs["reference_manifest_sha256"], digest);
    assert_eq!(outputs["cases"].as_object().unwrap().len(), 21);
    outputs
}
