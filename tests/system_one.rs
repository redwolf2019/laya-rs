use laya_server::system_one::{
    Answer, Criteria, Field, Limits, Request, RequestError, Response, RlAgent,
    json::{self, Value},
};

fn request(question: &str) -> Vec<u8> {
    format!(r#"{{"state":null,"questions":{{"q":{question}}}}}"#).into_bytes()
}

#[test]
fn complete_response_uses_official_fields_and_order() {
    let response = Response::new(
        vec![
            (
                "10".into(),
                Answer::Choice {
                    choice: "b".into(),
                    probabilities: vec![("b".into(), 0.75), ("a".into(), 0.25)],
                    confidence: 0.1887,
                    rl_agent: RlAgent {
                        act_probability: 0.123456789,
                    },
                },
            ),
            (
                "2".into(),
                Answer::Score {
                    score: 0.25,
                    legend: vec![("0".into(), "low".into()), ("1".into(), "high".into())],
                    probabilities: vec![("0".into(), 0.75), ("1".into(), 0.25)],
                    confidence: 0.1887,
                    rl_agent: RlAgent {
                        act_probability: 0.5,
                    },
                },
            ),
            (
                "".into(),
                Answer::Noul {
                    noul: 0.8,
                    rl_agent: RlAgent {
                        act_probability: 0.9,
                    },
                },
            ),
        ],
        42,
    );
    let encoded = serde_json::to_string(&response).unwrap();
    assert_eq!(
        encoded,
        concat!(
            r#"{"model":"rl-agent","answers":{"10":{"type":"choice","choice":"b","probabilities":{"b":0.75,"a":0.25},"confidence":0.1887,"rl_agent":{"act_probability":0.123456789}},"#,
            r#""2":{"type":"score","score":0.25,"legend":{"0":"low","1":"high"},"probabilities":{"0":0.75,"1":0.25},"confidence":0.1887,"rl_agent":{"act_probability":0.5}},"#,
            r#""":{"type":"noul","noul":0.8,"rl_agent":{"act_probability":0.9}}},"usage":{"input_tokens":42,"output_tokens":0}}"#,
        )
    );
}

#[test]
fn choice_normalization_preserves_first_position_and_last_description() {
    for (criteria, expected) in [
        (r#"["b","a","b"]"#, vec![("b", None), ("a", None)]),
        (
            r#"{"10":"old","2":null,"10":"new","":""}"#,
            vec![("10", Some("new")), ("2", None), ("", Some(""))],
        ),
    ] {
        let body = request(&format!(
            r#"{{"type":"choice","instructions":"选择","criteria":{criteria}}}"#
        ));
        let parsed = Request::from_slice(&body, &Limits::default()).unwrap();
        let Criteria::Choice(options) = &parsed.questions[0].criteria else {
            panic!("expected Choice");
        };
        let actual: Vec<_> = options
            .iter()
            .map(|option| (option.key.as_str(), option.description.as_deref()))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn score_keeps_repeated_levels_and_noul_defaults_are_false_then_true() {
    let cases = [
        (
            r#"{"type":"score","instructions":null,"criteria":["high","low","high"]}"#,
            vec!["level 0: high", "level 1: low", "level 2: high"],
        ),
        (
            r#"{"type":"noul","instructions":""}"#,
            vec![
                "false: no, the statement does not hold",
                "true: yes, the statement holds",
            ],
        ),
        (
            r#"{"type":"noul","instructions":false,"criteria":{}}"#,
            vec![
                "false: no, the statement does not hold",
                "true: yes, the statement holds",
            ],
        ),
        (
            r#"{"type":"noul","instructions":{},"criteria":{"false":"","true":"","ignored":null}}"#,
            vec![
                "false: no, the statement does not hold",
                "true: yes, the statement holds",
            ],
        ),
        (
            r#"{"type":"noul","instructions":1.5,"criteria":{"true":"成立"}}"#,
            vec!["false: no, the statement does not hold", "true: 成立"],
        ),
        (
            r#"{"type":"noul","instructions":[],"criteria":{"false":"不成立"}}"#,
            vec!["false: 不成立", "true: yes, the statement holds"],
        ),
        (
            r#"{"type":"noul","instructions":true,"criteria":{"true":"yes","false":"no"}}"#,
            vec!["false: no", "true: yes"],
        ),
        (
            r#"{"type":"choice","instructions":"","criteria":{"b":null,"a":"","c":"解释"}}"#,
            vec!["b", "a", "c: 解释"],
        ),
    ];
    for (question, expected) in cases {
        let parsed = Request::from_slice(&request(question), &Limits::default()).unwrap();
        assert_eq!(parsed.questions[0].criteria.option_texts(), expected);
    }
}

#[test]
fn state_and_instructions_accept_all_json_kinds_without_number_loss() {
    let large_integer = "9".repeat(4301);
    let values = [
        "null",
        "true",
        "false",
        r#""中文😄""#,
        "[]",
        "{}",
        "[1,null]",
        r#"{"10":1,"2":{"z":1,"a":2}}"#,
        "1",
        "-0",
        "1.0",
        "1e0",
        "-0.0",
        "1e-7",
        "9007199254740993",
        "18446744073709551616",
        "-9223372036854775809",
        "1e400",
        "-1e400",
        "1e-4000",
        "-1e-4000",
        &large_integer,
    ];
    for value in values {
        let body = format!(
            r#"{{"state":{value},"questions":{{"q":{{"type":"noul","instructions":{value}}}}}}}"#
        );
        let parsed = Request::from_slice(body.as_bytes(), &Limits::default()).unwrap();
        assert_eq!(parsed.state.get(), value);
        assert_eq!(parsed.questions[0].instructions.get(), value);
        match json::view(&parsed.state).unwrap() {
            Value::Integer(v) => assert_eq!(v, value),
            Value::Float(v) => assert_eq!(v, value),
            _ => {}
        }
    }
}

#[test]
fn ordered_views_preserve_nested_keys_and_numeric_categories() {
    let body = br#"{"state":{"10":1,"2":{"z":1,"a":2,"z":1.0},"10":1e0},"questions":{"q":{"type":"noul","instructions":[-0,-0.0]}}}"#;
    let parsed = Request::from_slice(body, &Limits::default()).unwrap();
    let Value::Object(entries) = json::view(&parsed.state).unwrap() else {
        panic!()
    };
    assert_eq!(
        entries.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["10", "2"]
    );
    assert!(matches!(
        json::view(entries[0].1).unwrap(),
        Value::Float("1e0")
    ));
    let Value::Object(nested) = json::view(entries[1].1).unwrap() else {
        panic!()
    };
    assert_eq!(
        nested.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["z", "a"]
    );
    assert!(matches!(
        json::view(nested[0].1).unwrap(),
        Value::Float("1.0")
    ));
    assert!(matches!(
        json::view(nested[1].1).unwrap(),
        Value::Integer("2")
    ));
    let Value::Array(numbers) = json::view(&parsed.questions[0].instructions).unwrap() else {
        panic!()
    };
    assert!(matches!(
        json::view(numbers[0]).unwrap(),
        Value::Integer("-0")
    ));
    assert!(matches!(
        json::view(numbers[1]).unwrap(),
        Value::Float("-0.0")
    ));
}

#[test]
fn effective_fields_use_last_values_but_questions_keep_first_positions() {
    let body = br#"{"state":0,"state":null,"questions":null,"questions":{
        "10":false,"2":{"type":"score","instructions":0,"criteria":["x","x"]},
        "10":{"type":5,"type":"noul","instructions":null,"criteria":{"false":null,"false":"no"}},
        "":{"type":"choice","instructions":true,"criteria":{"a":5,"a":null,"b":"b"}}
    },"ignored":{"any":[]}}"#;
    let parsed = Request::from_slice(body, &Limits::default()).unwrap();
    assert_eq!(parsed.state.get(), "null");
    assert_eq!(
        parsed
            .questions
            .iter()
            .map(|q| q.name.as_str())
            .collect::<Vec<_>>(),
        ["10", "2", ""]
    );
    assert_eq!(parsed.questions[0].criteria.option_texts()[0], "false: no");
}

#[test]
fn required_fields_and_question_types_are_strict() {
    use Field::*;
    use RequestError::*;
    for (body, expected) in [
        ("null", InvalidField(Root)),
        ("[]", InvalidField(Root)),
        (r#"{"questions":{}}"#, MissingField(State)),
        (r#"{"state":null}"#, MissingField(Questions)),
        (
            r#"{"state":null,"questions":null}"#,
            InvalidField(Questions),
        ),
        (r#"{"state":null,"questions":[]}"#, InvalidField(Questions)),
        (r#"{"state":null,"questions":{}}"#, QuestionCount),
    ] {
        assert_error(body.as_bytes(), expected);
    }
    for (question, expected) in [
        ("null", InvalidField(Question)),
        ("[]", InvalidField(Question)),
        ("{}", MissingField(Type)),
        (r#"{"type":null}"#, InvalidField(Type)),
        (r#"{"type":3}"#, InvalidField(Type)),
        (r#"{"type":"Choice"}"#, UnsupportedQuestionType),
        (r#"{"type":"secret body"}"#, UnsupportedQuestionType),
        (r#"{"type":"noul"}"#, MissingField(Instructions)),
        (
            r#"{"type":"choice","instructions":null}"#,
            MissingField(Criteria),
        ),
        (
            r#"{"type":"score","instructions":null}"#,
            MissingField(Criteria),
        ),
    ] {
        assert_error(&request(question), expected);
    }
}

fn assert_error(body: &[u8], expected: RequestError) {
    let error = Request::from_slice(body, &Limits::default()).unwrap_err();
    assert_eq!(error, expected);
    assert_eq!(error.status(), 400);
    assert_eq!(error.to_string(), "Invalid request");
    assert_eq!(
        serde_json::to_string(&error.envelope()).unwrap(),
        r#"{"error":{"code":"invalid_request","message":"Invalid request"}}"#
    );
}

#[test]
fn criteria_rejects_every_non_public_form() {
    let error = RequestError::InvalidField(Field::Criteria);
    for (kind, invalid) in [
        (
            "choice",
            vec![
                "null",
                "true",
                "3",
                r#""text""#,
                "[1,2]",
                "[null,\"a\"]",
                r#"{"a":1,"b":null}"#,
                r#"{"a":false,"b":null}"#,
            ],
        ),
        (
            "score",
            vec![
                "null",
                "true",
                "3",
                r#""text""#,
                "{}",
                "[null,null]",
                "[1,2]",
                "[{},[]]",
            ],
        ),
        (
            "noul",
            vec![
                "null",
                "true",
                "3",
                r#""text""#,
                "[]",
                r#"{"false":null}"#,
                r#"{"true":1}"#,
                r#"{"true":false}"#,
            ],
        ),
    ] {
        for criteria in invalid {
            let question =
                format!(r#"{{"type":"{kind}","instructions":null,"criteria":{criteria}}}"#);
            assert_error(&request(&question), error);
        }
    }
}

#[test]
fn too_few_effective_options_are_model_boundary_errors() {
    for (kind, criteria) in [
        ("choice", "[]"),
        ("choice", "{}"),
        ("choice", r#"["x","x"]"#),
        ("choice", r#"{"a":null,"a":"x"}"#),
        ("score", "[]"),
        ("score", r#"["x"]"#),
    ] {
        assert_error(
            &request(&format!(
                r#"{{"type":"{kind}","instructions":null,"criteria":{criteria}}}"#
            )),
            RequestError::TooFewOptions,
        );
    }
}

#[test]
fn syntax_and_unicode_are_checked_in_ignored_and_overwritten_values() {
    for value in [
        "NaN",
        "Infinity",
        "-Infinity",
        "01",
        "+1",
        "1.",
        "1e",
        "1e+",
        ".5",
        "[1,]",
        r#"{"x":1,}"#,
        r#"{"x" 1}"#,
        "/*comment*/null",
        "\u{feff}null",
        r#""\ud800""#,
        r#""\udc00""#,
        r#"{"\ud800":null}"#,
        "\"\n\"",
    ] {
        let body = format!(
            r#"{{"state":{value},"state":null,"questions":{{"q":{{"type":"noul","instructions":null}}}}}}"#
        );
        assert_error(body.as_bytes(), RequestError::JsonSyntax);
        let body = format!(
            r#"{{"state":null,"ignored":{value},"questions":{{"q":{{"type":"noul","instructions":null}}}}}}"#
        );
        assert_error(body.as_bytes(), RequestError::JsonSyntax);
    }
    let valid = request(r#"{"type":"noul","instructions":"\ud83d\ude04"}"#);
    let parsed = Request::from_slice(&valid, &Limits::default()).unwrap();
    assert!(
        matches!(json::view(&parsed.questions[0].instructions).unwrap(), Value::String(s) if s == "😄")
    );
    for suffix in [b"x".as_slice(), &[0xff], b"\x0b"] {
        let mut body = valid.clone();
        body.extend_from_slice(suffix);
        assert_error(&body, RequestError::JsonSyntax);
    }
    let mut body = vec![0xef, 0xbb, 0xbf];
    body.extend_from_slice(&valid);
    assert_error(&body, RequestError::JsonSyntax);
}

#[test]
fn question_limit_applies_after_duplicate_key_collapse() {
    for count in [1, 16, 17] {
        let entries = (0..count)
            .map(|i| format!(r#""{i}":{{"type":"noul","instructions":null}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let body = format!(r#"{{"state":null,"questions":{{{entries}}}}}"#);
        let result = Request::from_slice(body.as_bytes(), &Limits::default());
        if count <= 16 {
            assert_eq!(result.unwrap().questions.len(), count);
        } else {
            assert_eq!(result.unwrap_err(), RequestError::QuestionCount);
        }
    }
    let entries = vec![r#""q":{"type":"noul","instructions":null}"#; 17].join(",");
    let body = format!(r#"{{"state":null,"questions":{{{entries}}}}}"#);
    assert_eq!(
        Request::from_slice(body.as_bytes(), &Limits::default())
            .unwrap()
            .questions
            .len(),
        1
    );
}

#[test]
fn option_limits_cover_arrays_objects_and_duplicate_semantics() {
    for count in [2, 32, 33] {
        let array = format!(
            "[{}]",
            (0..count)
                .map(|i| format!(r#""{i}""#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let object = format!(
            "{{{}}}",
            (0..count)
                .map(|i| format!(r#""{i}":null"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        for (kind, criteria) in [("choice", &array), ("choice", &object), ("score", &array)] {
            let body = request(&format!(
                r#"{{"type":"{kind}","instructions":null,"criteria":{criteria}}}"#
            ));
            let result = Request::from_slice(&body, &Limits::default());
            if count <= 32 {
                assert_eq!(result.unwrap().questions[0].criteria.len(), count);
            } else {
                assert_eq!(result.unwrap_err(), RequestError::TooManyOptions);
            }
        }
    }
    let criteria = format!(r#"["b",{}]"#, vec![r#""a""#; 40].join(","));
    let body = request(&format!(
        r#"{{"type":"choice","instructions":null,"criteria":{criteria}}}"#
    ));
    assert_eq!(
        Request::from_slice(&body, &Limits::default())
            .unwrap()
            .questions[0]
            .criteria
            .len(),
        2
    );
    let body = request(&format!(
        r#"{{"type":"score","instructions":null,"criteria":{criteria}}}"#
    ));
    assert_error(&body, RequestError::TooManyOptions);
}

#[test]
fn full_body_byte_limit_includes_whitespace() {
    let limits = Limits::default();
    let mut body = request(r#"{"type":"noul","instructions":null}"#);
    body.resize(limits.max_body_bytes, b' ');
    assert!(Request::from_slice(&body, &limits).is_ok());
    body.push(b' ');
    let error = Request::from_slice(&body, &limits).unwrap_err();
    assert_eq!(error, RequestError::PayloadTooLarge);
    assert_eq!(error.status(), 413);
    assert_eq!(
        serde_json::to_string(&error.envelope()).unwrap(),
        r#"{"error":{"code":"payload_too_large","message":"Request body too large"}}"#
    );
}

#[test]
fn depth_is_checked_before_overwritten_or_unknown_values_are_discarded() {
    for (depth, accepted) in [(64, true), (65, false)] {
        let nested = format!("{}0{}", "[".repeat(depth - 1), "]".repeat(depth - 1));
        for field in ["state", "unknown"] {
            let body = format!(
                r#"{{"{field}":{nested},"state":null,"questions":{{"q":{{"type":"noul","instructions":null}}}}}}"#
            );
            let result = Request::from_slice(body.as_bytes(), &Limits::default());
            if accepted {
                assert!(result.is_ok());
            } else {
                assert_eq!(result.unwrap_err(), RequestError::JsonDepth);
            }
        }
    }
}

#[test]
fn explicit_limits_apply_without_the_serde_default_recursion_ceiling() {
    let mut limits = Limits {
        max_body_bytes: 4096,
        max_questions: 1,
        max_options: 2,
        max_json_depth: 256,
    };
    let nested = format!("{}0{}", "[".repeat(255), "]".repeat(255));
    let body = format!(
        r#"{{"state":{nested},"questions":{{"q":{{"type":"noul","instructions":null}}}}}}"#
    );
    assert!(Request::from_slice(body.as_bytes(), &limits).is_ok());
    limits.max_json_depth = 255;
    assert_eq!(
        Request::from_slice(body.as_bytes(), &limits).unwrap_err(),
        RequestError::JsonDepth
    );
    let body = request(r#"{"type":"score","instructions":null,"criteria":["a","b","c"]}"#);
    assert_eq!(
        Request::from_slice(&body, &limits).unwrap_err(),
        RequestError::TooManyOptions
    );
    let body = br#"{"state":0,"questions":{"a":{"type":"noul","instructions":0},"b":{"type":"noul","instructions":0}}}"#;
    assert_eq!(
        Request::from_slice(body, &limits).unwrap_err(),
        RequestError::QuestionCount
    );
}
