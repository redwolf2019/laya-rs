use super::*;

fn tensor(ty: TensorElementType, shape: &[i64], symbols: &[&str]) -> ValueType {
    ValueType::Tensor {
        ty,
        shape: shape.into(),
        dimension_symbols: symbols.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn session_interface_rejects_wrong_names_types_shapes_and_dynamic_symbols() {
    let logits = tensor(TensorElementType::Float32, &[-1, -1], &["batch", "options"]);
    let action = tensor(TensorElementType::Float32, &[-1, 2], &["batch", ""]);
    assert!(
        validate_interface(
            [("logits", &logits), ("act_probs", &action)].into_iter(),
            false
        )
        .is_ok()
    );
    for invalid in [
        tensor(TensorElementType::Int64, &[-1, -1], &["batch", "options"]),
        tensor(TensorElementType::Float32, &[1, -1], &["", "options"]),
        tensor(TensorElementType::Float32, &[-1], &["batch"]),
        tensor(TensorElementType::Float32, &[-1, -1], &["batch", "seq"]),
        ValueType::Sequence(Box::new(logits.clone())),
    ] {
        assert!(
            validate_interface(
                [("logits", &invalid), ("act_probs", &action)].into_iter(),
                false
            )
            .is_err()
        );
    }
}

#[test]
fn session_interface_rejects_wrong_names_counts_and_action_width() {
    let logits = tensor(TensorElementType::Float32, &[-1, -1], &["batch", "options"]);
    let action = tensor(TensorElementType::Float32, &[-1, 2], &["batch", ""]);
    assert!(
        validate_interface(
            [("logits", &logits), ("act_logits", &action)].into_iter(),
            false
        )
        .is_err()
    );
    assert!(validate_interface([("logits", &logits)].into_iter(), false).is_err());
    assert!(
        validate_interface(
            [("logits", &logits), ("act_probs", &action)].into_iter(),
            true
        )
        .is_err()
    );
    let wrong_action = tensor(TensorElementType::Float32, &[-1, 3], &["batch", ""]);
    assert!(
        validate_interface(
            [("logits", &logits), ("act_probs", &wrong_action)].into_iter(),
            false
        )
        .is_err()
    );
}

#[test]
fn bundle_rejects_missing_truncated_and_same_size_corrupted_files() {
    let root = std::env::temp_dir().join(format!("laya-bundle-test-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    assert!(verify_bundle(&root).unwrap_err().source().is_some());
    let file = File::create(root.join("laya.onnx")).unwrap();
    assert_eq!(
        verify_bundle(&root).unwrap_err().message,
        "bundle file size differs from manifest"
    );
    file.set_len(2_680_422).unwrap();
    assert_eq!(
        verify_bundle(&root).unwrap_err().message,
        "bundle file SHA-256 differs from manifest"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_native_library_returns_a_sanitized_error_without_panicking() {
    let error =
        initialize_runtime(Path::new("/missing/private-secret/libonnxruntime.so")).unwrap_err();
    assert_eq!(
        error.to_string(),
        "native ONNX Runtime load or ABI check failed"
    );
    assert!(error.source().is_some());
}

#[test]
#[ignore = "requires Linux, real #7 bundle and official ORT 1.28 CPU library; see docs/validation/model-loader.md"]
fn linux_real_model_smoke() {
    assert_eq!(std::env::consts::OS, "linux", "Linux acceptance only");
    let directory =
        std::env::var_os("LAYA_TEST_MODEL").expect("set LAYA_TEST_MODEL to the real bundle");
    let library =
        std::env::var_os("LAYA_TEST_ORT").expect("set LAYA_TEST_ORT to the official CPU library");
    let config = Config::from_args([
        "--model".into(),
        directory,
        "--ort-library".into(),
        library,
        "--threads".into(),
        "2".into(),
        "--inter-op-threads".into(),
        "2".into(),
    ])
    .unwrap();
    let mut model = Model::load(&config).unwrap();
    assert_eq!(model.sessions.len(), 2);
    assert!(model.sequence.tokenizer().get_truncation().is_none());
    assert!(model.sequence.tokenizer().get_padding().is_none());
    assert_eq!(
        model
            .sequence
            .tokenizer()
            .encode("<bos><eos><mask><pad>", false)
            .unwrap()
            .get_ids(),
        [2, 1, 4, 0]
    );
    assert_smoke_reference(&mut model);
    println!("{}", ort::info());
    println!(
        "PASS: verified bundle, CPUExecutionProvider, two reusable sessions, intra=2 inter=2, tensor-L2 reference tolerance"
    );
}

#[test]
fn model_config_preserves_default_type_and_bucket_temperatures() {
    let defaults = ModelConfig::from_slice(br#"{"max_len":1024,"head_max_len":256}"#).unwrap();
    let score = laya_server::system_one::Criteria::Score(vec![String::new(); 3]);
    let noul = laya_server::system_one::Criteria::Noul {
        false_description: String::new(),
        true_description: String::new(),
    };
    assert_eq!(defaults.calibration.temperature(&score), 1.0);
    assert_eq!(defaults.calibration.temperature(&noul), 1.0);
    let custom = ModelConfig::from_slice(br#"{"max_len":1024,"head_max_len":256,"temperature":[0.5,2,3],"temperature_by_options":{"choice:2":0.25}}"#).unwrap();
    let choice = laya_server::system_one::Criteria::Choice(vec![
        laya_server::system_one::ChoiceOption {
            key: "a".into(),
            description: None,
        },
        laya_server::system_one::ChoiceOption {
            key: "b".into(),
            description: None,
        },
    ]);
    assert_eq!(custom.calibration.temperature(&choice), 0.25);
    assert_eq!(custom.calibration.temperature(&score), 2.0);
    assert_eq!(custom.calibration.temperature(&noul), 3.0);
}

#[test]
fn model_config_validates_all_temperatures_and_task_budgets() {
    let base = serde_json::json!({"max_len":1024,"head_max_len":256});
    for (key, value) in [
        ("max_len", serde_json::json!(8192)),
        ("head_max_len", serde_json::json!(0)),
        ("temperature", serde_json::json!([1, 0, 1])),
        ("temperature", serde_json::json!([1, -1, 1])),
        ("temperature", serde_json::json!([1, 1])),
        ("temperature", serde_json::Value::Null),
        ("temperature_by_options", serde_json::Value::Null),
        ("temperature_by_options", serde_json::json!({"unused":0})),
    ] {
        let mut invalid = base.clone();
        invalid[key] = value;
        assert!(
            ModelConfig::from_slice(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "{invalid}"
        );
    }
    for invalid in [
        b"{".as_slice(),
        br#"{"max_len":1024,"head_max_len":256,"temperature":[1,NaN,1]}"#,
        br#"{"max_len":1024,"head_max_len":256,"temperature":[1,1e999,1]}"#,
    ] {
        assert!(ModelConfig::from_slice(invalid).is_err());
    }
}

fn assert_smoke_reference(model: &mut Model) {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/validation/model-prep/validation.json"
    ))
    .unwrap();
    let case = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "tensor-L2")
        .unwrap();
    for (slot, session) in model.sessions.iter_mut().enumerate() {
        let actual = smoke(session).unwrap();
        for (head, key, atol, rtol) in [
            (0, "reference_logits", 1e-4, 1e-3),
            (1, "reference_act_probs", 1e-5, 1e-4),
        ] {
            for (observed, expected) in actual[head].iter().zip(case[key][0].as_array().unwrap()) {
                let expected = expected.as_f64().unwrap();
                assert!((f64::from(*observed) - expected).abs() <= atol + rtol * expected.abs());
            }
        }
        println!(
            "slot={slot} repeated tensor-L2 inference: logits={:?}, act_probs={:?}",
            actual[0], actual[1]
        );
    }
}
