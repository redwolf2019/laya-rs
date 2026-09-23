//! Real Linux CPU regression: fixed requests -> Rust sequence -> ORT -> typed answers.
use super::*;
use laya_server::{
    postprocess::Outputs,
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
}

fn check_case(model: &mut Model, data: &Value, name: &str, slot: usize) {
    let request = Request::from_slice(
        data["request_json"].as_str().unwrap().as_bytes(),
        &Limits::default(),
    )
    .unwrap();
    let batch = model.sequence.build(&request).unwrap();
    let tokens = batch.usage.input_tokens;
    let (shape, logits, action) = run(&mut model.sessions[slot], batch);
    check_probabilities(
        &model.config.calibration,
        &request,
        data,
        &logits,
        &action,
        name,
    );
    let actual = model
        .config
        .calibration
        .response(
            &request,
            Outputs {
                logits_shape: &shape,
                logits: &logits,
                act_probs_shape: &[shape[0], 2],
                act_probs: &action,
            },
            tokens,
        )
        .unwrap();
    check_answers(
        serde_json::to_value(actual).unwrap(),
        data["response"].clone(),
        name,
    );
}

fn run(session: &mut Session, batch: Batch) -> (Vec<i64>, Vec<f32>, Vec<f32>) {
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
    (shape.to_vec(), logits.to_vec(), action.to_vec())
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

fn check_answers(mut actual: Value, mut expected: Value, name: &str) {
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
