use super::*;
use crate::{scheduler::Scheduler, system_one::Limits};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Mutex, mpsc},
    task::Poll,
    time::Duration,
};
use tower::ServiceExt;

use crate::test_http as http;
mod network;

const VALID: &str = r#"{"state":"private-secret","questions":{"q":{"type":"noul","instructions":"private-question"}}}"#;

fn answer(_: system_one::Request) -> Result<system_one::Response, scheduler::Error> {
    Ok(system_one::Response::new(
        vec![(
            "q".into(),
            system_one::Answer::Noul {
                noul: 0.75,
                rl_agent: system_one::RlAgent {
                    act_probability: 0.12345678,
                },
            },
        )],
        42,
    ))
}

async fn pending<F: Future>(mut future: Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn successful_response_preserves_all_engine_fields_and_repeated_requests() {
    let mut scheduler = Scheduler::test_worker(1, 1, answer);
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    for media in [
        "application/json",
        "application/json; charset=utf-8",
        "APPLICATION/JSON; charset=\"UTF-8\"; profile=test",
    ] {
        let mut request = post(VALID);
        request
            .headers_mut()
            .insert("content-type", media.parse().unwrap());
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200, "media={media}");
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({
                "model":"rl-agent", "answers":{"q":{"type":"noul","noul":0.75,"rl_agent":{"act_probability":0.12345678}}},
                "usage":{"input_tokens":42,"output_tokens":0}
            })
        );
    }
    client.close();
    scheduler.run().await.unwrap();
    assert_eq!(client.snapshot().tracked_tasks, 0);
}

#[tokio::test]
async fn field_count_depth_and_unicode_rejections_have_the_same_envelope() {
    let mut scheduler = Scheduler::test_worker(1, 1, |_| unreachable!());
    let client = scheduler.client();
    let app = router(
        client.clone(),
        Limits {
            max_questions: 1,
            max_options: 2,
            max_json_depth: 4,
            ..Limits::default()
        },
    );
    for body in [
        r#"{"state":0,"questions":{}}"#,
        r#"{"state":0,"questions":{"a":{"type":"noul","instructions":0},"b":{"type":"noul","instructions":0}}}"#,
        r#"{"state":0,"questions":{"q":{"type":"choice","instructions":0,"criteria":["a","a"]}}}"#,
        r#"{"state":0,"questions":{"q":{"type":"score","instructions":0,"criteria":["a","b","c"]}}}"#,
        r#"{"state":[[[[0]]]],"questions":{}}"#,
        r#"{"state":"\ud800","questions":{}}"#,
        r#"{"state":0,"questions":{"q":{"type":"unknown","instructions":0}}}"#,
        r#"{"state":0,"questions":{"q":{"type":"noul"}}}"#,
    ] {
        error(
            app.clone().oneshot(post(body)).await.unwrap(),
            400,
            "invalid_request",
            "Invalid request",
        )
        .await;
    }
    client.close();
    scheduler.run().await.unwrap();
}

#[tokio::test]
async fn body_exact_limit_accepts_and_one_extra_byte_rejects() {
    let mut scheduler = Scheduler::test_worker(1, 0, answer);
    let client = scheduler.client();
    let app = router(
        client.clone(),
        Limits {
            max_body_bytes: VALID.len(),
            ..Limits::default()
        },
    );
    assert_eq!(
        app.clone().oneshot(post(VALID)).await.unwrap().status(),
        200
    );
    error(
        app.oneshot(post(format!("{VALID} "))).await.unwrap(),
        413,
        "payload_too_large",
        "Request body too large",
    )
    .await;
    client.close();
    scheduler.run().await.unwrap();
}

#[tokio::test]
async fn model_failure_is_sanitized_and_marker_loss_remains_a_client_error() {
    use crate::{engine, sequence};
    for (failure, status, code, message) in [
        (
            engine::Error::Sequence(sequence::Error::Tokenizer(
                std::io::Error::other("/private/model private-secret").into(),
            )),
            500,
            "inference_failed",
            "Inference failed",
        ),
        (
            engine::Error::Sequence(sequence::Error::MarkerLost),
            400,
            "invalid_request",
            "Invalid request",
        ),
    ] {
        let failure = Mutex::new(Some(failure));
        let mut scheduler = Scheduler::test_worker(1, 0, move |_| {
            Err(scheduler::Error::Inference(
                failure.lock().unwrap().take().unwrap(),
            ))
        });
        let client = scheduler.client();
        let app = router(client.clone(), Limits::default());
        error(
            app.oneshot(post(VALID)).await.unwrap(),
            status,
            code,
            message,
        )
        .await;
        client.close();
        scheduler.run().await.unwrap();
        assert_eq!(client.snapshot().inflight, 0);
    }
}

fn blocked_worker(
    queue: usize,
) -> (
    Scheduler,
    mpsc::Sender<()>,
    tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    let (release, gate) = mpsc::channel();
    let gate = Mutex::new(gate);
    let (started, starts) = tokio::sync::mpsc::unbounded_channel();
    let scheduler = Scheduler::test_worker(1, queue, move |request| {
        started.send(()).unwrap();
        gate.lock().unwrap().recv().unwrap();
        answer(request)
    });
    (scheduler, release, starts)
}

#[tokio::test(start_paused = true)]
async fn queue_full_queue_timeout_and_execution_timeout_keep_cpu_slot() {
    let (mut scheduler, release, mut starts) = blocked_worker(1);
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let mut first = Box::pin(app.clone().oneshot(post(VALID)));
    pending(first.as_mut()).await;
    starts.recv().await.unwrap();
    let mut second = Box::pin(app.clone().oneshot(post(VALID)));
    pending(second.as_mut()).await;
    error(
        app.clone().oneshot(post(VALID)).await.unwrap(),
        429,
        "queue_full",
        "Inference queue full",
    )
    .await;
    assert_ready(app.clone()).await;
    tokio::time::advance(Duration::from_secs(3)).await;
    error(
        second.await.unwrap(),
        503,
        "queue_timeout",
        "Inference queue timeout",
    )
    .await;
    tokio::time::advance(Duration::from_secs(2)).await;
    error(
        first.await.unwrap(),
        504,
        "inference_timeout",
        "Inference wait timeout",
    )
    .await;
    assert_eq!(
        (client.snapshot().inflight, client.snapshot().waiting),
        (1, 0)
    );
    assert_timeout_metrics(&app).await;
    assert!(starts.try_recv().is_err());
    client.close();
    release.send(()).unwrap();
    scheduler.run().await.unwrap();
    assert_eq!(
        (client.snapshot().inflight, client.snapshot().tracked_tasks),
        (0, 0)
    );
    let metrics = scrape(&app).await;
    assert!(metrics.contains("laya_request_duration_seconds_count 3\n"));
    assert!(metrics.contains("laya_inference_inflight 0\n"));
}

async fn assert_timeout_metrics(app: &Router) {
    let metrics = scrape(app).await;
    for sample in [
        "laya_requests_total 3",
        "laya_request_duration_seconds_count 3",
        "laya_request_duration_seconds_sum 8.0",
        "laya_inference_inflight 1",
        "laya_queue_size 0",
        "laya_errors_total{reason=\"queue_full\"} 1",
        "laya_errors_total{reason=\"queue_timeout\"} 1",
        "laya_errors_total{reason=\"inference_timeout\"} 1",
    ] {
        assert!(
            metrics.lines().any(|line| line == sample),
            "missing {sample}: {metrics}"
        );
    }
}

#[tokio::test]
async fn cancelling_route_futures_removes_queue_but_keeps_running_work() {
    let (mut scheduler, release, mut starts) = blocked_worker(1);
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let mut first = Box::pin(app.clone().oneshot(post(VALID)));
    pending(first.as_mut()).await;
    starts.recv().await.unwrap();
    let mut second = Box::pin(app.clone().oneshot(post(VALID)));
    pending(second.as_mut()).await;
    assert_eq!(client.snapshot().waiting, 1);
    drop(second);
    drop(first);
    assert_eq!(
        (client.snapshot().waiting, client.snapshot().inflight),
        (0, 1)
    );
    let mut queued = Box::pin(app.oneshot(post(VALID)));
    pending(queued.as_mut()).await;
    client.close();
    error(
        queued.await.unwrap(),
        503,
        "unavailable",
        "Service unavailable",
    )
    .await;
    assert!(starts.try_recv().is_err());
    release.send(()).unwrap();
    scheduler.run().await.unwrap();
    assert_eq!(
        (
            client.snapshot().waiting,
            client.snapshot().inflight,
            client.snapshot().tracked_tasks
        ),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn metrics_and_unknown_routes_use_documented_transport_responses() {
    let mut scheduler = Scheduler::test_worker(1, 0, |_| unreachable!());
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let metrics = app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(metrics.status(), 200);
    assert_eq!(
        metrics.headers()["content-type"],
        "application/openmetrics-text; version=1.0.0; charset=utf-8"
    );
    let text =
        String::from_utf8(to_bytes(metrics.into_body(), 16384).await.unwrap().to_vec()).unwrap();
    assert!(text.contains("# TYPE laya_requests counter\n"));
    assert!(text.contains("laya_requests_total 0\n"));
    assert!(text.contains("# TYPE laya_request_duration_seconds histogram\n"));
    assert!(text.contains("# TYPE laya_inference_duration_seconds histogram\n"));
    assert!(text.contains("laya_queue_size 0\n"));
    assert!(text.contains("laya_inference_inflight 0\n"));
    assert!(text.ends_with("# EOF\n"));
    assert_unknown_routes(&app).await;
    client.close();
    scheduler.run().await.unwrap();
}

async fn assert_unknown_routes(app: &Router) {
    for (method, path, status, allow) in [
        ("GET", "/private-secret", 404, None),
        ("POST", "/healthz", 405, Some("GET,HEAD")),
        ("GET", "/v1/system-one", 405, Some("POST")),
        ("HEAD", "/healthz", 200, None),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers().get("allow").map(|v| v.to_str().unwrap()),
            allow
        );
        assert!(
            to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

async fn error(response: axum::response::Response, status: u16, code: &str, message: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"error":{"code":code,"message":message}})
    );
}

fn post(body: impl Into<Body>) -> Request<Body> {
    Request::post("/v1/system-one")
        .header("content-type", "application/json")
        .body(body.into())
        .unwrap()
}

#[tokio::test]
async fn rejects_media_stream_overflow_and_json_before_inference() {
    use futures_util::{StreamExt, stream};
    let mut scheduler = Scheduler::test_worker(1, 1, |_| unreachable!("rejected input ran"));
    let client = scheduler.client();
    let app = router(
        client.clone(),
        Limits {
            max_body_bytes: 8,
            ..Limits::default()
        },
    );
    let request = Request::post("/v1/system-one")
        .body(Body::from_stream(stream::pending::<
            Result<String, std::io::Error>,
        >()))
        .unwrap();
    error(
        app.clone().oneshot(request).await.unwrap(),
        415,
        "unsupported_media_type",
        "Unsupported media type",
    )
    .await;
    // No size hint or Content-Length, and never reaches EOF: reject on byte 9.
    let chunks =
        stream::iter([Ok::<_, std::io::Error>("1234"), Ok("56789")]).chain(stream::pending());
    error(
        app.clone()
            .oneshot(post(Body::from_stream(chunks)))
            .await
            .unwrap(),
        413,
        "payload_too_large",
        "Request body too large",
    )
    .await;
    error(
        app.clone().oneshot(post("{secret")).await.unwrap(),
        400,
        "invalid_request",
        "Invalid request",
    )
    .await;
    client.close();
    scheduler.run().await.unwrap();
}

async fn assert_ready(app: Router) {
    let response = app
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn unavailable_precedes_media_and_body_validation() {
    let mut scheduler = Scheduler::test_worker(1, 0, |_| unreachable!());
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    client.close();
    let request = Request::post("/v1/system-one")
        .body(Body::from_stream(futures_util::stream::pending::<
            Result<String, std::io::Error>,
        >()))
        .unwrap();
    error(
        app.oneshot(request).await.unwrap(),
        503,
        "unavailable",
        "Service unavailable",
    )
    .await;
    scheduler.run().await.unwrap();
}

#[tokio::test]
async fn closing_admission_during_body_read_prevents_execution() {
    let mut scheduler = Scheduler::test_worker(1, 0, |_| unreachable!());
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let (send, receive) = tokio::sync::oneshot::channel();
    let body = Body::from_stream(futures_util::stream::once(async {
        receive.await.unwrap();
        Ok::<_, std::io::Error>(VALID)
    }));
    let mut request = Box::pin(app.oneshot(post(body)));
    pending(request.as_mut()).await;
    client.close();
    send.send(()).unwrap();
    error(
        request.await.unwrap(),
        503,
        "unavailable",
        "Service unavailable",
    )
    .await;
    scheduler.run().await.unwrap();
}

#[tokio::test]
async fn bad_media_and_body_io_errors_never_echo_diagnostics() {
    let mut scheduler = Scheduler::test_worker(1, 0, |_| unreachable!());
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    for media in [
        "text/plain",
        "text/json",
        "private-invalid",
        "application/vnd.test+json",
        "application/json; charset=iso-8859-1",
        "application/json; charset=utf-8; charset=utf-16",
    ] {
        let mut request = post(VALID);
        request
            .headers_mut()
            .insert("content-type", media.parse().unwrap());
        error(
            app.clone().oneshot(request).await.unwrap(),
            415,
            "unsupported_media_type",
            "Unsupported media type",
        )
        .await;
    }
    let body = Body::from_stream(futures_util::stream::iter([Err::<String, _>(
        std::io::Error::other("private-secret /private/model"),
    )]));
    error(
        app.oneshot(post(body)).await.unwrap(),
        400,
        "invalid_request",
        "Invalid request",
    )
    .await;
    client.close();
    scheduler.run().await.unwrap();
}

#[tokio::test]
async fn health_and_readiness_follow_resource_admission() {
    let mut scheduler =
        Scheduler::test_worker(1, 1, |_| unreachable!("health cannot run inference"));
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    for (path, status, body) in [
        ("/healthz", 200, r#"{"status":"ok"}"#),
        ("/readyz", 200, r#"{"status":"ready"}"#),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), body);
    }
    client.close();
    let response = app
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        r#"{"error":{"code":"unavailable","message":"Service unavailable"}}"#
    );
    scheduler.run().await.unwrap();
}

async fn scrape(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    String::from_utf8(
        to_bytes(response.into_body(), 32768)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn metrics_count_request_terminals_once_and_follow_real_slots() {
    let (mut scheduler, release, mut starts) = blocked_worker(1);
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let mut running = Box::pin(app.clone().oneshot(post(VALID)));
    pending(running.as_mut()).await;
    starts.recv().await.unwrap();
    let mut queued = Box::pin(app.clone().oneshot(post(VALID)));
    pending(queued.as_mut()).await;
    let text = scrape(&app).await;
    assert!(text.contains("laya_requests_total 2\n"));
    assert!(text.contains("laya_queue_size 1\n"));
    assert!(text.contains("laya_inference_inflight 1\n"));
    drop(queued);
    drop(running);
    let text = scrape(&app).await;
    assert!(text.contains("laya_errors_total{reason=\"client_cancelled\"} 2\n"));
    assert!(text.contains("laya_request_duration_seconds_count 2\n"));
    assert!(text.contains("laya_queue_size 0\n"));
    assert!(text.contains("laya_inference_inflight 1\n"));
    release.send(()).unwrap();
    client.close();
    scheduler.run().await.unwrap();
    let text = scrape(&app).await;
    assert!(text.contains("laya_inference_inflight 0\n"));
    // Controlled CPU work isn't a Session run; never invent native observations.
    assert!(text.contains("laya_inference_duration_seconds_count 0\n"));
    assert!(text.contains("laya_errors_total{reason=\"client_cancelled\"} 2\n"));
}

#[tokio::test]
async fn metrics_count_success_and_all_validation_failures_without_client_labels() {
    let mut scheduler = Scheduler::test_worker(1, 0, answer);
    let client = scheduler.client();
    let app = router(
        client.clone(),
        Limits {
            max_body_bytes: VALID.len(),
            ..Limits::default()
        },
    );
    for (body, status) in [
        (VALID.to_owned(), 200),
        ("secret-malformed".into(), 400),
        (format!("{VALID} "), 413),
    ] {
        assert_eq!(
            app.clone().oneshot(post(body)).await.unwrap().status(),
            status
        );
    }
    let request = Request::post("/v1/system-one")
        .body(Body::from(VALID))
        .unwrap();
    assert_eq!(app.clone().oneshot(request).await.unwrap().status(), 415);
    client.close();
    assert_eq!(
        app.clone().oneshot(post(VALID)).await.unwrap().status(),
        503
    );
    scheduler.run().await.unwrap();
    let text = scrape(&app).await;
    for reason in [
        "invalid_request",
        "payload_too_large",
        "unsupported_media_type",
        "unavailable",
    ] {
        assert!(
            text.contains(&format!("laya_errors_total{{reason=\"{reason}\"}} 1\n")),
            "{text}"
        );
    }
    assert!(text.contains("laya_requests_total 5\n"));
    assert!(text.contains("laya_request_duration_seconds_count 5\n"));
    assert!(!text.contains("secret"));
    assert!(!text.contains("reason=\"success\""));
}

#[tokio::test(start_paused = true)]
async fn late_inference_failure_cannot_count_a_second_http_error() {
    let (release, gate) = mpsc::channel();
    let gate = Mutex::new(gate);
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let mut scheduler = Scheduler::test_worker(1, 0, move |_| {
        started.send(()).unwrap();
        gate.lock().unwrap().recv().unwrap();
        Err(scheduler::Error::Inference(
            crate::engine::Error::MissingOutput,
        ))
    });
    let client = scheduler.client();
    let app = router(client.clone(), Limits::default());
    let mut request = Box::pin(app.clone().oneshot(post(VALID)));
    pending(request.as_mut()).await;
    starts.recv().await.unwrap();
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(request.await.unwrap().status(), 504);
    let before = scrape(&app).await;
    assert!(before.contains("laya_inference_inflight 1\n"));
    release.send(()).unwrap();
    client.close();
    scheduler.run().await.unwrap();
    let after = scrape(&app).await;
    assert!(after.contains("laya_errors_total{reason=\"inference_timeout\"} 1\n"));
    assert!(!after.contains("reason=\"inference_failed\""));
    assert!(after.contains("laya_request_duration_seconds_count 1\n"));
    assert!(after.contains("laya_inference_inflight 0\n"));
}
