//! Exact sequence oracle; only the explicit ignored test needs the local tokenizer.

use std::{fs, path::PathBuf};

use laya_server::{
    sequence::{Error, SequenceBuilder},
    system_one::{Limits, Request},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

fn real_builder() -> SequenceBuilder {
    let root = PathBuf::from(std::env::var_os("LAYA_TEST_MODEL").expect("set LAYA_TEST_MODEL"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../docs/model-manifest.json")).unwrap();
    for name in [
        "tokenizer/tokenizer.json",
        "tokenizer/tokenizer_config.json",
        "laya_config.json",
    ] {
        let entry = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == name)
            .unwrap();
        let digest: String = Sha256::digest(fs::read(root.join(name)).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(digest, entry["sha256"].as_str().unwrap(), "{name}");
    }
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("laya_config.json")).unwrap()).unwrap();
    SequenceBuilder::new(
        Tokenizer::from_file(root.join("tokenizer/tokenizer.json")).unwrap(),
        &fs::read(root.join("tokenizer/tokenizer_config.json")).unwrap(),
        config["max_len"].as_u64().unwrap().try_into().unwrap(),
        config["head_max_len"].as_u64().unwrap().try_into().unwrap(),
    )
    .unwrap()
}

#[derive(Deserialize)]
struct Fixture {
    request_json: String,
    tensors: Tensors,
    response: serde_json::Value,
    rows: Vec<FixtureRow>,
}

#[derive(Deserialize)]
struct FixtureRow {
    tokenizer_calls: Vec<TokenCall>,
}

#[derive(Deserialize)]
struct TokenCall {
    text: String,
    input_ids: Vec<u32>,
}

#[derive(Deserialize)]
struct Tensors {
    input_ids: Vec<Vec<i64>>,
    attention_mask: Vec<Vec<i64>>,
    marker_pos: Vec<Vec<i64>>,
    marker_mask: Vec<Vec<bool>>,
    qtype: Vec<i64>,
}

#[test]
#[ignore = "requires the pinned local tokenizer; set LAYA_TEST_MODEL (no ONNX weights/runtime needed)"]
fn real_tokenizer_matches_all_official_sequence_fixtures() {
    let builder = real_builder();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/system-one");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut count = 0;
    for file in manifest["files"].as_array().unwrap() {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(file["path"].as_str().unwrap())).unwrap())
                .unwrap();
        if value["kind"] == "official_model" {
            eprintln!("checking {}", file["path"]);
            check_fixture(&builder, serde_json::from_value(value).unwrap());
            count += 1;
        }
    }
    assert_eq!(count, 21);
}

fn check_fixture(builder: &SequenceBuilder, fixture: Fixture) {
    for row in fixture.rows {
        for call in row.tokenizer_calls {
            let encoding = builder.tokenizer().encode(call.text, false).unwrap();
            assert_eq!(encoding.get_ids(), call.input_ids);
        }
    }
    let request = Request::from_slice(fixture.request_json.as_bytes(), &Limits::default()).unwrap();
    let batch = builder.build(&request).unwrap();
    assert_eq!(
        batch.token_shape,
        [
            fixture.tensors.qtype.len(),
            fixture.tensors.input_ids[0].len()
        ]
    );
    assert_eq!(
        batch.marker_shape,
        [
            fixture.tensors.qtype.len(),
            fixture.tensors.marker_pos[0].len()
        ]
    );
    assert_eq!(batch.input_ids, fixture.tensors.input_ids.concat());
    assert_eq!(
        batch.attention_mask,
        fixture.tensors.attention_mask.concat()
    );
    assert_eq!(batch.marker_pos, fixture.tensors.marker_pos.concat());
    assert_eq!(batch.marker_mask, fixture.tensors.marker_mask.concat());
    assert_eq!(batch.qtype, fixture.tensors.qtype);
    assert_eq!(
        serde_json::to_value(batch.usage).unwrap(),
        fixture.response["usage"]
    );
}

#[derive(Deserialize)]
struct Boundary {
    name: String,
    max_len: usize,
    head_max_len: usize,
    request_json: String,
    input_ids: Vec<i64>,
    marker_pos: Vec<i64>,
    expected_error: Option<String>,
}

#[test]
#[ignore = "requires the pinned local tokenizer; set LAYA_TEST_MODEL"]
fn real_tokenizer_matches_synthetic_budget_oracle() {
    let loaded = real_builder();
    let root = PathBuf::from(std::env::var_os("LAYA_TEST_MODEL").unwrap());
    let config = fs::read(root.join("tokenizer/tokenizer_config.json")).unwrap();
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/sequence-boundaries.json")).unwrap();
    let cases: Vec<Boundary> = serde_json::from_value(oracle["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        eprintln!("checking {}", case.name);
        let builder = SequenceBuilder::new(
            loaded.tokenizer().clone(),
            &config,
            case.max_len,
            case.head_max_len,
        )
        .unwrap();
        let request =
            Request::from_slice(case.request_json.as_bytes(), &Limits::default()).unwrap();
        if let Some(error) = case.expected_error {
            assert_eq!(error, "marker_lost");
            assert!(matches!(builder.build(&request), Err(Error::MarkerLost)));
        } else {
            let batch = builder.build(&request).unwrap();
            assert_eq!(batch.token_shape, [1, case.input_ids.len()]);
            assert_eq!(batch.marker_shape, [1, 2]);
            assert_eq!(batch.input_ids, case.input_ids);
            assert_eq!(batch.marker_pos, case.marker_pos);
            assert_eq!(batch.attention_mask, vec![1; case.input_ids.len()]);
            assert_eq!(batch.marker_mask, [true, true]);
            assert_eq!(batch.qtype, [0]);
            assert_eq!(
                serde_json::to_value(batch.usage).unwrap(),
                serde_json::json!({"input_tokens":case.input_ids.len(),"output_tokens":0})
            );
        }
    }
}

#[test]
#[ignore = "requires the pinned local tokenizer; set LAYA_TEST_MODEL"]
fn maximum_request_resources_preserve_every_question_and_option() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/system-one/options-32.json")).unwrap();
    // This fixture contains only simple strings; reconstructing its JSON is lossless.
    let mut value: serde_json::Value = serde_json::from_str(&fixture.request_json).unwrap();
    let question = value["questions"]["q"].clone();
    value["questions"] = (0..16).map(|i| (i.to_string(), question.clone())).collect();
    let mut nested = serde_json::Value::Null;
    for _ in 0..63 {
        nested = serde_json::json!([nested]);
    }
    value["ignored"] = nested;
    value["padding"] = "".into();
    let remaining = 1_048_576 - serde_json::to_vec(&value).unwrap().len();
    value["padding"] = " ".repeat(remaining).into();
    let body = serde_json::to_vec(&value).unwrap();
    assert_eq!(body.len(), 1_048_576);
    let request = Request::from_slice(&body, &Limits::default()).unwrap();
    let batch = real_builder().build(&request).unwrap();
    assert_eq!(batch.token_shape, [16, fixture.tensors.input_ids[0].len()]);
    assert_eq!(batch.marker_shape, [16, 32]);
    assert_eq!(batch.input_ids, fixture.tensors.input_ids[0].repeat(16));
    assert_eq!(
        batch.attention_mask,
        fixture.tensors.attention_mask[0].repeat(16)
    );
    assert_eq!(batch.marker_pos, fixture.tensors.marker_pos[0].repeat(16));
    assert_eq!(batch.marker_mask, vec![true; 16 * 32]);
    assert_eq!(batch.qtype, vec![0; 16]);
    assert_eq!(
        batch.usage.input_tokens,
        fixture.response["usage"]["input_tokens"].as_u64().unwrap() as usize * 16
    );
}

const TEST_CONFIG: &[u8] =
    br#"{"cls_token":"<cls>","sep_token":"<sep>","mask_token":"<mask>","pad_token":"<pad>"}"#;

fn tiny_tokenizer() -> Tokenizer {
    let words = [
        "<unk>",
        "<pad>",
        "<cls>",
        "<sep>",
        "<mask>",
        "choice",
        "question:",
        "a",
        "b",
        "state",
    ];
    let model = tokenizers::models::wordlevel::WordLevel::builder()
        .vocab(
            words
                .into_iter()
                .enumerate()
                .map(|(i, s)| (s.to_owned(), u32::try_from(i).unwrap()))
                .collect(),
        )
        .unk_token("<unk>".into())
        .build()
        .unwrap();
    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_pre_tokenizer(Some(
        tokenizers::pre_tokenizers::whitespace::WhitespaceSplit,
    ));
    tokenizer
}

#[test]
fn batch_records_explicit_markers_and_counts_repeated_state_without_padding() {
    let builder = SequenceBuilder::new(tiny_tokenizer(), TEST_CONFIG, 1024, 256).unwrap();
    let request = Request::from_slice(br#"{"state":"state","questions":{"a":{"type":"choice","instructions":"<mask><mask>","criteria":["a","b"]},"b":{"type":"choice","instructions":"<mask><mask>","criteria":["a","b","<mask>"]}}}"#, &Limits::default()).unwrap();
    let batch = builder.build(&request).unwrap();
    assert_eq!(batch.token_shape, [2, 12]);
    assert_eq!(batch.marker_shape, [2, 3]);
    assert_eq!(
        batch.input_ids,
        [
            2, 5, 6, 3, 4, 7, 4, 8, 3, 9, 3, 1, 2, 5, 6, 3, 4, 7, 4, 8, 4, 3, 9, 3
        ]
    );
    assert_eq!(
        batch.attention_mask,
        [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
        ]
    );
    assert_eq!(batch.marker_pos, [4, 6, 0, 4, 6, 8]);
    assert_eq!(batch.marker_mask, [true, true, false, true, true, true]);
    assert_eq!(batch.qtype, [0, 0]);
    assert_eq!(
        serde_json::to_value(batch.usage).unwrap(),
        serde_json::json!({"input_tokens":23,"output_tokens":0})
    );
}

#[test]
fn marker_at_truncation_boundary_rejects_the_whole_request() {
    let request = Request::from_slice(br#"{"state":"","questions":{"valid":{"type":"choice","instructions":"","criteria":["a","b"]},"lost":{"type":"choice","instructions":"","criteria":["a","b","state"]}}}"#, &Limits::default()).unwrap();
    let builder = SequenceBuilder::new(tiny_tokenizer(), TEST_CONFIG, 8, 256).unwrap();
    assert!(matches!(builder.build(&request), Err(Error::MarkerLost)));
    let builder = SequenceBuilder::new(tiny_tokenizer(), TEST_CONFIG, 9, 256).unwrap();
    let batch = builder.build(&request).unwrap();
    assert_eq!(batch.marker_pos, [4, 6, 0, 4, 6, 8]);
    // The final option's marker survives alone; the reference does not repair SEP.
    assert_eq!(batch.input_ids[17], 4);
}

#[test]
fn invalid_budgets_and_special_token_configuration_return_typed_errors() {
    for (max_len, head) in [(0, 256), (usize::MAX, 256), (1024, usize::MAX)] {
        assert!(matches!(
            SequenceBuilder::new(tiny_tokenizer(), TEST_CONFIG, max_len, head),
            Err(Error::InvalidConfiguration)
        ));
    }
    assert!(matches!(
        SequenceBuilder::new(tiny_tokenizer(), b"{}", 1024, 256),
        Err(Error::Configuration(_))
    ));
    let bad = String::from_utf8(TEST_CONFIG.to_vec())
        .unwrap()
        .replace("<mask>", "missing-secret");
    let result = SequenceBuilder::new(tiny_tokenizer(), bad.as_bytes(), 1024, 256);
    assert!(matches!(result, Err(Error::InvalidConfiguration)));
}
