//! Real Linux HTTP acceptance; synthetic route tests do not prove model parity.
use super::*;
use laya_server::{
    api, engine,
    scheduler::{Client, Scheduler},
    system_one::{Limits, Request},
};
use serde_json::Value;
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, time::timeout};

struct NativeLogs(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

impl tracing::Subscriber for NativeLogs {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "laya_server::model"
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = self.0.lock().unwrap();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                fields.push(format!("{field}={value:?}"));
            },
        );
    }
}

#[path = "../../../tests/support/http.rs"]
mod wire;

#[test]
#[ignore = "Linux real bundle/ORT; set LAYA_TEST_MODEL and LAYA_TEST_ORT"]
fn linux_http_model_parity_and_disconnect() {
    assert_eq!(std::env::consts::OS, "linux");
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    tracing::subscriber::set_global_default(NativeLogs(logs.clone())).unwrap();
    let config = Config::from_args([
        "--model".into(),
        std::env::var_os("LAYA_TEST_MODEL").unwrap(),
        "--ort-library".into(),
        std::env::var_os("LAYA_TEST_ORT").unwrap(),
        "--threads".into(),
        "2".into(),
        "--max-concurrency".into(),
        "1".into(),
    ])
    .unwrap();
    let mut model = Model::load(&config).unwrap();
    let fixtures = engine_responses(&mut model);
    model.sequence = super::parity::sequence_with_untrained_token(&model);
    let scheduler = Scheduler::new(
        model.sessions,
        model.sequence,
        model.config.calibration,
        8,
        Duration::from_secs(30),
        Duration::from_secs(120),
    )
    .unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(check_http(scheduler, fixtures));
    let logs = logs.lock().unwrap();
    assert!(
        logs.iter().any(|v| v == "level=Error"),
        "native failure must use the sanitized logger"
    );
    assert!(
        logs.iter()
            .all(|v| v == "event=\"native_runtime\"" || v.starts_with("level="))
    );
    println!("PASS: native log fields contain only a static event and typed severity");
}

fn engine_responses(model: &mut Model) -> Vec<Value> {
    let root = Path::new("tests/fixtures/system-one");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut fixtures = Vec::new();
    for entry in manifest["files"].as_array().unwrap() {
        let name = entry["path"].as_str().unwrap();
        let mut data: Value =
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        if data["kind"] != "official_model" {
            continue;
        }
        let request = Request::from_slice(
            data["request_json"].as_str().unwrap().as_bytes(),
            &Limits::default(),
        )
        .unwrap();
        let response = engine::system_one(
            &request,
            &model.sequence,
            &mut model.sessions[0],
            &model.config.calibration,
        )
        .unwrap();
        let response = serde_json::to_value(response).unwrap();
        super::parity::check_answers(response.clone(), data["response"].clone(), name);
        data["engine_response"] = response;
        data["fixture"] = name.into();
        fixtures.push(data);
    }
    assert_eq!(fixtures.len(), 21);
    fixtures
}

async fn check_http(mut scheduler: Scheduler, fixtures: Vec<Value>) {
    let client = scheduler.client();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = api::router(client.clone(), Limits::default());
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                stopped.await.unwrap();
            })
            .await
            .unwrap();
    });
    let owner = tokio::spawn(async move {
        scheduler.run().await.unwrap();
    });
    check_probes(address).await;
    for data in &fixtures {
        check_response(address, data).await;
    }
    let mixed = fixtures
        .iter()
        .find(|d| d["fixture"] == "mixed-6.json")
        .unwrap();
    check_repeats(address, mixed).await;
    chunked_overflow(address).await;
    disconnect(address, &client, mixed).await;
    native_error(address).await;
    check_response(address, mixed).await;
    let (_, _, metrics) = wire::get(address, "/metrics").await;
    let metrics = std::str::from_utf8(&metrics).unwrap();
    assert!(
        metrics.contains("laya_inference_duration_seconds_count 28\n"),
        "{metrics}"
    );
    assert!(metrics.contains("laya_requests_total 29\n"), "{metrics}");
    assert!(metrics.contains("laya_errors_total{reason=\"inference_failed\"} 1\n"));
    assert!(metrics.contains("laya_inference_inflight 0\n"));
    println!("METRICS after real runs:\n{metrics}");
    client.close();
    assert_eq!(wire::get(address, "/readyz").await.0, 503);
    owner.await.unwrap();
    let snapshot = client.snapshot();
    assert_eq!(
        (snapshot.waiting, snapshot.inflight, snapshot.tracked_tasks),
        (0, 0, 0)
    );
    stop.send(()).unwrap();
    server.await.unwrap();
    println!(
        "PASS: 21 HTTP/engine/official fixtures, 4 concurrent repeats, chunked 413, TCP disconnect, recovery, all tasks reaped"
    );
}

async fn check_repeats(address: SocketAddr, data: &Value) {
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..4 {
        let data = data.clone();
        callers.spawn(async move {
            check_response(address, &data).await;
        });
    }
    while let Some(result) = callers.join_next().await {
        result.unwrap();
    }
}

async fn native_error(address: SocketAddr) {
    let raw = r#"{"state":"engine_probe_untrained_token","questions":{"q":{"type":"noul","instructions":"private-instructions"}}}"#;
    let (status, _, body) = wire::receive(wire::post(address, raw).await).await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        serde_json::json!({"error":{"code":"inference_failed","message":"Inference failed"}})
    );
    println!("PASS: real native inference failure returns a static 500 envelope");
}

async fn check_probes(address: SocketAddr) {
    for (path, expected) in [("/healthz", "ok"), ("/readyz", "ready")] {
        let (status, _, body) = wire::get(address, path).await;
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({"status": expected})
        );
    }
    let (status, headers, body) = wire::get(address, "/metrics").await;
    assert_eq!(status, 200);
    assert!(headers.contains("application/openmetrics-text"));
    assert!(
        std::str::from_utf8(&body)
            .unwrap()
            .contains("laya_requests_total 0\n")
    );
}

async fn check_response(address: SocketAddr, data: &Value) {
    let socket = wire::post(address, data["request_json"].as_str().unwrap()).await;
    let (status, _, body) = wire::receive(socket).await;
    assert_eq!(status, 200);
    let response: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        response == data["engine_response"],
        "HTTP response differs from same-input engine (text redacted)"
    );
    super::parity::check_answers(
        response,
        data["response"].clone(),
        data["fixture"].as_str().unwrap(),
    );
    println!("PASS HTTP/engine/official: {}", data["fixture"]);
}

async fn chunked_overflow(address: SocketAddr) {
    let limit = Limits::default().max_body_bytes;
    let wire = format!(
        "POST /v1/system-one HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n1\r\nx\r\n",
        limit,
        "x".repeat(limit)
    );
    let (status, _, body) = wire::receive(wire::send(address, wire.as_bytes()).await).await;
    assert_eq!(status, 413);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"],
        "payload_too_large"
    );
    println!("PASS: chunked body >1 MiB rejected before terminal chunk");
}

async fn disconnect(address: SocketAddr, client: &Client, data: &Value) {
    let socket = wire::post(address, data["request_json"].as_str().unwrap()).await;
    timeout(Duration::from_secs(10), async {
        while client.snapshot().inflight == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(socket);
    println!(
        "TCP closed during real work: inflight={}",
        client.snapshot().inflight
    );
    timeout(Duration::from_secs(30), async {
        while client.snapshot().inflight != 0 || client.snapshot().tracked_tasks != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    println!("PASS: disconnected CPU work completed and owner reaped it");
}
