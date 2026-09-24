//! Process-level CLI checks; no model or native runtime is loaded.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_laya-server"))
        .env("LAYA_API_TOKEN", "test-only-api-token")
        .args(args)
        .output()
        .expect("CLI process must run")
}

#[test]
fn missing_or_invalid_token_fails_before_model_loading_without_echo() {
    for token in [
        None,
        Some(""),
        Some("private-secret token"),
        Some("private-secret\n"),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_laya-server"));
        command.args(["--model", "."]).env_remove("LAYA_API_TOKEN");
        if let Some(token) = token {
            command.env("LAYA_API_TOKEN", token);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("LAYA_API_TOKEN"));
        assert!(!error.contains("private-secret"));
        assert!(!error.contains("Model initialization"));
    }
}

#[test]
fn help_runs_without_model_and_lists_the_frozen_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_laya-server"))
        .arg("--help")
        .env_remove("LAYA_API_TOKEN")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    for flag in [
        "--model",
        "--ort-library",
        "--listen",
        "--threads",
        "--inter-op-threads",
        "--max-concurrency",
        "--max-body-bytes",
        "--max-questions",
        "--max-options",
        "--max-json-depth",
        "--queue-capacity",
        "--queue-timeout",
        "--inference-timeout",
        "--shutdown-grace",
        "LAYA_API_TOKEN",
    ] {
        assert!(help.contains(flag), "missing {flag}");
    }
}

#[cfg(unix)]
#[test]
fn non_unicode_token_is_a_sanitized_configuration_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let output = Command::new(env!("CARGO_BIN_EXE_laya-server"))
        .args(["--model", "."])
        .env("LAYA_API_TOKEN", OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Invalid configuration: LAYA_API_TOKEN must contain a valid Bearer token\n"
    );
}

#[test]
fn invalid_input_fails_without_echoing_values() {
    for args in [
        vec![],
        vec!["--model"],
        vec!["--model", "/missing/private-secret-model"],
        vec!["--model", "Cargo.toml"],
        vec!["--model", ".", "--listen", "private-secret-host:8080"],
        vec!["--model", ".", "--listen", "127.0.0.1:0"],
        vec!["--model", ".", "--listen", "127.0.0.1:65536"],
        vec!["--model", ".", "--private-secret-flag", "value"],
        vec!["--model", ".", "private-secret-positional"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.starts_with("Invalid configuration:"), "{error}");
        assert!(!error.contains("private-secret"), "{error}");
    }
}

#[test]
fn missing_bundle_fails_before_native_loading_or_listening() {
    let output = run(&[
        "--model",
        ".",
        "--listen",
        "[::1]:8081",
        "--queue-capacity",
        "0",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Model initialization failed: bundle file is missing or unreadable")
    );
}

#[test]
fn integer_options_reject_invalid_and_unrepresentable_values() {
    for flag in [
        "--threads",
        "--inter-op-threads",
        "--max-concurrency",
        "--max-body-bytes",
        "--max-questions",
        "--max-options",
        "--max-json-depth",
        "--queue-capacity",
        "--queue-timeout",
        "--inference-timeout",
        "--shutdown-grace",
    ] {
        for value in [
            "-1",
            "1.5",
            "private-secret",
            "",
            "99999999999999999999999999999",
            "0",
        ] {
            if flag == "--queue-capacity" && value == "0" {
                continue;
            }
            let output = run(&["--model", ".", flag, value]);
            assert_eq!(output.status.code(), Some(2), "{flag} {value}");
            assert!(
                !String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("private-secret")
            );
        }
    }
}

#[test]
fn native_and_clock_bounds_are_checked() {
    for (flag, value) in [
        ("--max-options", "1"),
        ("--threads", "2147483648"),
        ("--inter-op-threads", "2147483648"),
        ("--queue-timeout", "18446744073709551615"),
        ("--inference-timeout", "18446744073709551615"),
        ("--shutdown-grace", "18446744073709551615"),
    ] {
        assert_eq!(run(&["--model", ".", flag, value]).status.code(), Some(2));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_input_is_rejected_without_a_panic_or_echo() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let output = Command::new(env!("CARGO_BIN_EXE_laya-server"))
        .args(["--model", ".", "--threads"])
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr).is_ok());
}
