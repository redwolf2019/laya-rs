//! Offline integrity/schema and request-normalization checks for the #10 oracle.
//! These do not claim Rust sequence building, model inference or postprocessing parity.

use std::{collections::BTreeSet, fs, path::Path};

use laya_server::system_one::{Limits, Request};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    status: String,
    files: Vec<FixtureFile>,
}

#[derive(Deserialize)]
struct FixtureFile {
    path: String,
    sha256: String,
}

#[derive(Deserialize)]
struct ModelFixture {
    request_json: String,
    state_text: String,
    rows: Vec<Row>,
    tensors: Tensors,
    logits: Vec<Vec<f32>>,
    act_logits: Vec<Vec<f32>>,
    act_probs: Vec<Vec<f32>>,
    response: serde_json::Value,
    onnx: serde_json::Value,
    comparison: serde_json::Value,
}

#[derive(Deserialize)]
struct Row {
    name: String,
    option_keys: Vec<String>,
    option_texts: Vec<String>,
    instructions_text: String,
    tokenizer_calls: Vec<TokenCall>,
    length: usize,
    probabilities: Vec<f32>,
    scaled_logits: Vec<f32>,
    temperature: serde_json::Value,
}

#[derive(Deserialize)]
struct TokenCall {
    text: String,
    input_ids: Vec<u32>,
}

#[derive(Deserialize)]
struct Tensors {
    input_ids: Vec<Vec<u32>>,
    attention_mask: Vec<Vec<u8>>,
    marker_pos: Vec<Vec<usize>>,
    marker_mask: Vec<Vec<bool>>,
    qtype: Vec<u8>,
}

#[test]
fn official_fixtures_are_integral_and_consumable_without_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/system-one");
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.schema_version, 1);
    // A useful oracle must preserve failures; #10 found a real rounded Score mismatch.
    assert_eq!(manifest.status, "failed");
    let listed: BTreeSet<_> = manifest.files.iter().map(|f| f.path.clone()).collect();
    let actual: BTreeSet<_> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".json") && n != "manifest.json")
        .collect();
    assert_eq!(listed, actual);
    let mut models = 0;
    for entry in manifest.files {
        let bytes = fs::read(root.join(&entry.path)).unwrap();
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, entry.sha256, "{}", entry.path);
        let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fixture["schema_version"], 1);
        match fixture["kind"].as_str().unwrap() {
            "official_model" => {
                check_model(serde_json::from_value(fixture).unwrap());
                models += 1;
            }
            "contract_rejection" => check_rejected(&fixture),
            "artificial_logits" => check_artificial(&fixture),
            "provenance" => {
                assert_eq!(
                    fixture["official_revision"],
                    "c5d78730f3493e4fe16d61507ef4b78eef7318cf"
                );
                assert_eq!(
                    fixture["checkpoint_revision"],
                    "052592a15d198d9ad47da779604259b10b47b7aa"
                );
            }
            kind => panic!("unexpected fixture kind: {kind}"),
        }
    }
    assert_eq!(models, 21);
}

fn check_model(fixture: ModelFixture) {
    let request = Request::from_slice(fixture.request_json.as_bytes(), &Limits::default()).unwrap();
    assert_eq!(
        request.state_text().unwrap().as_bytes(),
        fixture.state_text.as_bytes()
    );
    let b = request.questions.len();
    let t = &fixture.tensors;
    assert_eq!(b, fixture.rows.len());
    for length in [
        t.input_ids.len(),
        t.attention_mask.len(),
        t.marker_pos.len(),
        t.marker_mask.len(),
        t.qtype.len(),
        fixture.logits.len(),
        fixture.act_logits.len(),
        fixture.act_probs.len(),
    ] {
        assert_eq!(length, b);
    }
    for (i, (question, row)) in request.questions.iter().zip(&fixture.rows).enumerate() {
        assert_eq!(question.name, row.name);
        assert_eq!(
            question.instructions_text().unwrap().as_bytes(),
            row.instructions_text.as_bytes()
        );
        assert_eq!(question.criteria.option_texts(), row.option_texts);
        check_row(&fixture, i);
    }
    let usage: usize = fixture.rows.iter().map(|r| r.length).sum();
    assert_eq!(fixture.response["usage"]["input_tokens"], usage);
    assert_eq!(fixture.response["usage"]["output_tokens"], 0);
    assert_eq!(fixture.response["model"], "rl-agent");
    assert!(fixture.onnx["response"].is_object());
    assert!(fixture.comparison["passed"].is_boolean());
    assert!(!fixture.state_text.is_empty());
}

fn check_row(fixture: &ModelFixture, i: usize) {
    let row = &fixture.rows[i];
    let t = &fixture.tensors;
    let k = row.option_texts.len();
    let width = t.input_ids[0].len();
    assert!((2..=32).contains(&k) && row.length <= 1024);
    assert_eq!(t.input_ids[i].len(), width);
    assert_eq!(t.attention_mask[i].len(), width);
    assert!(t.attention_mask[i][..row.length].iter().all(|v| *v == 1));
    assert!(t.attention_mask[i][row.length..].iter().all(|v| *v == 0));
    assert!(t.input_ids[i][row.length..].iter().all(|v| *v == 0));
    assert_eq!(t.marker_pos[i].len(), t.marker_mask[i].len());
    assert_eq!(fixture.logits[i].len(), t.marker_mask[i].len());
    assert!(t.marker_mask[i][..k].iter().all(|v| *v));
    assert!(t.marker_mask[i][k..].iter().all(|v| !*v));
    for pos in &t.marker_pos[i][..k] {
        assert!(*pos < row.length);
        assert_eq!(t.input_ids[i][*pos], 4);
    }
    assert!(t.qtype[i] <= 2);
    assert_eq!(row.option_keys.len(), k);
    assert_eq!(row.probabilities.len(), k);
    assert_eq!(row.scaled_logits.len(), k);
    assert_eq!(fixture.act_probs[i].len(), 2);
    assert_eq!(fixture.act_logits[i].len(), 2);
    check_values_and_text(fixture, i);
}

fn check_values_and_text(fixture: &ModelFixture, i: usize) {
    let row = &fixture.rows[i];
    assert!(
        fixture.logits[i]
            .iter()
            .chain(&fixture.act_logits[i])
            .chain(&row.scaled_logits)
            .all(|v| v.is_finite())
    );
    assert!(
        row.probabilities
            .iter()
            .chain(&fixture.act_probs[i])
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    );
    assert!((row.probabilities.iter().sum::<f32>() - 1.0).abs() <= 0.00011);
    assert!(row.temperature["value"].as_f64().unwrap() > 0.0);
    assert_eq!(row.tokenizer_calls.len(), row.option_texts.len() + 2);
    assert!(
        row.tokenizer_calls
            .iter()
            .all(|c| !c.text.contains("<mask>") && c.input_ids.iter().all(|id| *id < 256000))
    );
    assert!(row.tokenizer_calls[0].text.contains(" question: "));
    // Text may be empty; requiring its field during deserialization is intentional.
    let _ = &row.instructions_text;
}

fn check_rejected(fixture: &serde_json::Value) {
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        let bytes: Vec<u8> = serde_json::from_value(case["request_bytes"].clone()).unwrap();
        let error = Request::from_slice(&bytes, &Limits::default()).unwrap_err();
        assert_eq!(error.status(), case["status"].as_u64().unwrap() as u16);
        assert_eq!(error.envelope().error.code, case["code"].as_str().unwrap());
    }
}

fn check_artificial(fixture: &serde_json::Value) {
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 30);
    assert_eq!(fixture["rounding"][0]["expected"], 0.0312);
    assert_eq!(fixture["rounding"][1]["expected"], 0.0312);
    assert_eq!(fixture["rounding"][2]["expected"], 0.0313);
    assert_eq!(
        fixture["boundaries"][0]["response"]["answers"]["q"]["choice"],
        "0"
    );
    assert_eq!(
        fixture["boundaries"][1]["response"]["answers"]["q"]["choice"],
        "1"
    );
    let uniform = &fixture["boundaries"][3]["response"]["answers"]["q"]["probabilities"];
    assert!(uniform.as_object().unwrap().values().all(|v| *v == 0.0312));
}
