//! Real Linux CPU regression: fixed requests -> Rust sequence -> ORT -> typed answers.
use super::*;
use laya_server::{
    engine,
    sequence::Batch,
    system_one::{Limits, Request},
};
use serde_json::Value;

#[test]
#[ignore = "requires Linux, verified bundle and ORT; set LAYA_TEST_MODEL / LAYA_TEST_ORT"]
fn linux_system_one_model_parity() {
    assert_eq!(std::env::consts::OS, "linux");
    let config = Config::from_args([
        "--model".into(),
        std::env::var_os("LAYA_TEST_MODEL").unwrap(),
        "--ort-library".into(),
        std::env::var_os("LAYA_TEST_ORT").unwrap(),
    ])
    .unwrap();
    let mut model = Model::load(&config).unwrap();
    let root = Path::new("tests/fixtures/system-one");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut count = 0;
    for entry in manifest["files"].as_array().unwrap() {
        let name = entry["path"].as_str().unwrap();
        let data: Value = serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        if data["kind"] != "official_model" {
            continue;
        }
        let slot = count % model.sessions.len();
        check_case(&mut model, &data, name, slot);
        println!("PASS: {name}, slot={slot}");
        count += 1;
    }
    assert_eq!(count, 21);
    check_failures_and_reuse(&mut model);
}

fn check_failures_and_reuse(model: &mut Model) {
    use laya_server::system_one::RequestError;
    let valid = choice_request(2);
    let mut invalid = choice_request(2);
    invalid.questions.clear();
    let error = engine::system_one(
        &invalid,
        &model.sequence,
        &mut model.sessions[0],
        &model.config.calibration,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        engine::Error::Sequence(laya_server::sequence::Error::Request(
            RequestError::QuestionCount
        ))
    ));
    assert!(error.source().is_some());
    check_output_failures(model, &valid);
    check_native_failure(model, &valid);
}

fn sequence_with_untrained_token(model: &Model) -> SequenceBuilder {
    let mut tokenizer = model.sequence.tokenizer().clone();
    assert_eq!(
        tokenizer
            .add_tokens([tokenizers::AddedToken::from(
                "engine_probe_untrained_token",
                false
            )])
            .unwrap(),
        1
    );
    SequenceBuilder::new(
        tokenizer,
        &std::fs::read(
            Path::new(&std::env::var_os("LAYA_TEST_MODEL").unwrap())
                .join("tokenizer/tokenizer_config.json"),
        )
        .unwrap(),
        model.config.max_len,
        model.config.head_max_len,
    )
    .unwrap()
}

fn check_native_failure(model: &mut Model, valid: &Request) {
    let sequence = sequence_with_untrained_token(model);
    let bad = Request::from_slice(br#"{"state":"engine_probe_untrained_token","questions":{"q":{"type":"noul","instructions":""}}}"#, &Limits::default()).unwrap();
    let before = serde_json::to_value(
        engine::system_one(
            valid,
            &model.sequence,
            &mut model.sessions[0],
            &model.config.calibration,
        )
        .unwrap(),
    )
    .unwrap();
    let error = engine::system_one(
        &bad,
        &sequence,
        &mut model.sessions[0],
        &model.config.calibration,
    )
    .unwrap_err();
    assert!(matches!(error, engine::Error::Runtime(_)));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<ort::Error>()
            .is_some()
    );
    assert_eq!(error.envelope().error.code, "inference_failed");
    let after = serde_json::to_value(
        engine::system_one(
            valid,
            &model.sequence,
            &mut model.sessions[0],
            &model.config.calibration,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        before == after,
        "normal request differs after a real ORT failure"
    );
    println!("PASS: typed sequence/native failures; same real Session recovers without reloading");
}

fn choice_request(options: usize) -> Request {
    let raw = serde_json::json!({"state":"", "questions":{"q":{"type":"choice","instructions":"", "criteria": (0..options).map(|i| i.to_string()).collect::<Vec<_>>()}}});
    Request::from_slice(&serde_json::to_vec(&raw).unwrap(), &Limits::default()).unwrap()
}

fn check_output_failures(model: &Model, valid: &Request) {
    // Artificial graph: constant width=2; qtype=Noul emits NaN. No learned weights.
    let mut session = Session::builder()
        .unwrap()
        .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])
        .unwrap()
        .commit_from_memory(include_bytes!("../../../tests/fixtures/engine-probe.onnx"))
        .unwrap();
    let noul = Request::from_slice(
        br#"{"state":"","questions":{"q":{"type":"noul","instructions":""}}}"#,
        &Limits::default(),
    )
    .unwrap();
    for (request, expected) in [
        (choice_request(3), laya_server::postprocess::Error::Shape),
        (noul, laya_server::postprocess::Error::NonFinite),
    ] {
        let error = engine::system_one(
            &request,
            &model.sequence,
            &mut session,
            &model.config.calibration,
        )
        .unwrap_err();
        assert!(matches!(error, engine::Error::Output(e) if e == expected));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<laya_server::postprocess::Error>(),
            Some(&expected)
        );
        assert_eq!(error.status(), 500);
        let result = engine::system_one(
            valid,
            &model.sequence,
            &mut session,
            &model.config.calibration,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["answers"]["q"]["probabilities"]["0"],
            0.5
        );
    }
    println!("PASS: artificial native shape/NaN outputs rejected; same probe Session recovers");
}

fn check_case(model: &mut Model, data: &Value, name: &str, slot: usize) {
    let request = Request::from_slice(
        data["request_json"].as_str().unwrap().as_bytes(),
        &Limits::default(),
    )
    .unwrap();
    let batch = model.sequence.build(&request).unwrap();
    check_sequence(&batch, data, name);
    let (logits, action) = run(&mut model.sessions[slot], batch);
    check_probabilities(
        &model.config.calibration,
        &request,
        data,
        &logits,
        &action,
        name,
    );
    let actual = engine::system_one(
        &request,
        &model.sequence,
        &mut model.sessions[slot],
        &model.config.calibration,
    )
    .unwrap();
    check_answers(
        serde_json::to_value(actual).unwrap(),
        data["response"].clone(),
        name,
    );
}

fn run(session: &mut Session, batch: Batch) -> (Vec<f32>, Vec<f32>) {
    let outputs = session.run(ort::inputs! {
        "input_ids" => Tensor::from_array((batch.token_shape, batch.input_ids)).unwrap(),
        "attention_mask" => Tensor::from_array((batch.token_shape, batch.attention_mask)).unwrap(),
        "marker_pos" => Tensor::from_array((batch.marker_shape, batch.marker_pos)).unwrap(),
        "marker_mask" => Tensor::from_array((batch.marker_shape, batch.marker_mask)).unwrap(),
        "qtype" => Tensor::from_array(([batch.marker_shape[0]], batch.qtype)).unwrap(),
    }).unwrap();
    let (shape, logits) = outputs
        .get("logits")
        .unwrap()
        .try_extract_tensor::<f32>()
        .unwrap();
    let (act_shape, action) = outputs
        .get("act_probs")
        .unwrap()
        .try_extract_tensor::<f32>()
        .unwrap();
    assert_eq!(shape.as_ref(), batch.marker_shape.map(|v| v as i64));
    assert_eq!(act_shape.as_ref(), [shape[0], 2]);
    (logits.to_vec(), action.to_vec())
}

fn check_sequence(batch: &Batch, data: &Value, name: &str) {
    let tensors = &data["tensors"];
    assert_eq!(
        batch.token_shape,
        [
            tensors["input_ids"].as_array().unwrap().len(),
            tensors["input_ids"][0].as_array().unwrap().len()
        ]
    );
    assert_eq!(
        batch.marker_shape,
        [
            tensors["marker_pos"].as_array().unwrap().len(),
            tensors["marker_pos"][0].as_array().unwrap().len()
        ]
    );
    assert_eq!(
        batch.usage.input_tokens,
        data["response"]["usage"]["input_tokens"].as_u64().unwrap() as usize
    );
    for (field, actual) in [
        ("input_ids", serde_json::to_value(&batch.input_ids).unwrap()),
        (
            "attention_mask",
            serde_json::to_value(&batch.attention_mask).unwrap(),
        ),
        (
            "marker_pos",
            serde_json::to_value(&batch.marker_pos).unwrap(),
        ),
        (
            "marker_mask",
            serde_json::to_value(&batch.marker_mask).unwrap(),
        ),
        ("qtype", serde_json::to_value(&batch.qtype).unwrap()),
    ] {
        check_buffer(&actual, &tensors[field], name, field);
    }
}

fn check_buffer(actual: &Value, expected: &Value, name: &str, field: &str) {
    let expected: Vec<_> = if field == "qtype" {
        expected.as_array().unwrap().iter().collect()
    } else {
        expected
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|row| row.as_array().unwrap())
            .collect()
    };
    assert_eq!(
        actual.as_array().unwrap().len(),
        expected.len(),
        "{name}/{field}/length"
    );
    for (i, (a, b)) in actual.as_array().unwrap().iter().zip(expected).enumerate() {
        assert!(a == b, "{name}/{field}[{i}]: tensor mismatch");
    }
}

fn close(actual: f64, expected: f64, atol: f64, rtol: f64, location: &str) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!(
        (actual - expected).abs() <= atol + rtol * expected.abs(),
        "{location}: actual={actual}, expected={expected}"
    );
}

fn check_probabilities(
    calibration: &Calibration,
    request: &Request,
    data: &Value,
    logits: &[f32],
    action: &[f32],
    name: &str,
) {
    let k = logits.len() / request.questions.len();
    for (i, (q, row)) in request
        .questions
        .iter()
        .zip(logits.chunks_exact(k))
        .enumerate()
    {
        for (j, v) in row.iter().take(q.criteria.len()).enumerate() {
            close(
                f64::from(*v),
                data["logits"][i][j].as_f64().unwrap(),
                1e-4,
                1e-3,
                &format!("{name}/row[{i}]/logits[{j}]"),
            );
        }
        for (j, v) in calibration
            .probabilities(&q.criteria, row)
            .unwrap()
            .iter()
            .enumerate()
        {
            close(
                f64::from(*v),
                data["rows"][i]["probabilities"][j].as_f64().unwrap(),
                1e-5,
                1e-4,
                &format!("{name}/row[{i}]/probabilities[{j}]"),
            );
        }
    }
    for (i, v) in action.iter().enumerate() {
        close(
            f64::from(*v),
            data["act_probs"][i / 2][i % 2].as_f64().unwrap(),
            1e-5,
            1e-4,
            &format!("{name}/act_probs[{i}]"),
        );
    }
}

pub(super) fn check_answers(mut actual: Value, mut expected: Value, name: &str) {
    for (i, ((key, a), (ekey, e))) in actual["answers"]
        .as_object_mut()
        .unwrap()
        .iter_mut()
        .zip(expected["answers"].as_object_mut().unwrap().iter_mut())
        .enumerate()
    {
        assert!(key == ekey, "{name}/answer[{i}]: key mismatch");
        let aa = a.as_object_mut().unwrap().remove("rl_agent").unwrap();
        let ea = e.as_object_mut().unwrap().remove("rl_agent").unwrap();
        close(
            aa["act_probability"].as_f64().unwrap(),
            ea["act_probability"].as_f64().unwrap(),
            1e-5,
            1e-4,
            &format!("{name}/answer[{i}]/act_probability"),
        );
        for field in [
            "type",
            "choice",
            "score",
            "confidence",
            "probabilities",
            "legend",
            "noul",
        ] {
            if a[field].is_number() && e[field].is_number() {
                assert_eq!(
                    a[field].as_f64(),
                    e[field].as_f64(),
                    "{name}/answer[{i}]/{field}"
                );
            }
            assert!(
                a[field] == e[field],
                "{name}/answer[{i}]/{field}: mismatch (text redacted)"
            );
        }
    }
    assert!(
        actual == expected,
        "{name}: envelope or fields mismatch (text redacted)"
    );
}
