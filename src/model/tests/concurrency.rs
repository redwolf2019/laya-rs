//! Independent Linux processes for N=1/2: actual Session::run intervals and RSS.
use super::*;
use laya_server::{
    scheduler::{Client, Scheduler},
    system_one::{Limits, Request},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tracing::{
    Event, Metadata, Subscriber,
    span::{Attributes, Id, Record},
};

#[derive(Default)]
struct Runs {
    next: AtomicU64,
    events: Mutex<Vec<(Instant, u64, bool)>>,
}

struct Observer(Arc<Runs>);

impl Subscriber for Observer {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.name() == "laya_session_run"
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.0.next.fetch_add(1, Ordering::Relaxed) + 1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, _: &Event<'_>) {}
    fn enter(&self, id: &Id) {
        self.0
            .events
            .lock()
            .unwrap()
            .push((Instant::now(), id.into_u64(), true));
    }
    fn exit(&self, id: &Id) {
        self.0
            .events
            .lock()
            .unwrap()
            .push((Instant::now(), id.into_u64(), false));
    }
}

fn rss(stage: &str) {
    for line in std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
    {
        if line.starts_with("VmRSS:") || line.starts_with("VmHWM:") {
            println!("{stage}: {line}");
        }
    }
}

#[test]
#[ignore = "Linux real bundle/ORT; LAYA_TEST_CONCURRENCY=1 or 2; run in separate processes"]
fn linux_scheduler_concurrency() {
    assert_eq!(std::env::consts::OS, "linux");
    let config = test_config();
    assert!([1, 2].contains(&config.max_concurrency));
    rss("before_load");
    let model = Model::load(&config).unwrap();
    rss("loaded");
    let mut scheduler = Scheduler::new(
        model.sessions,
        model.sequence,
        model.config.calibration,
        config.queue_capacity,
        config.queue_timeout,
        config.inference_timeout,
    )
    .unwrap();
    let runs = Arc::new(Runs::default());
    tracing::subscriber::set_global_default(Observer(runs.clone())).unwrap();
    let client = scheduler.client();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let start = Instant::now();
    runtime.block_on(async {
        let (joined, ()) = tokio::join!(scheduler.run(), requests(client.clone()));
        joined.unwrap();
    });
    println!("stress_elapsed_ms={}", start.elapsed().as_millis());
    rss("after_stress");
    let snapshot = client.snapshot();
    assert!(!snapshot.accepting);
    assert_eq!(
        (snapshot.waiting, snapshot.inflight, snapshot.tracked_tasks),
        (0, 0, 0)
    );
    check_overlap(&runs, config.max_concurrency);
}

fn test_config() -> Config {
    Config::from_args([
        "--model".into(),
        std::env::var_os("LAYA_TEST_MODEL").unwrap(),
        "--ort-library".into(),
        std::env::var_os("LAYA_TEST_ORT").unwrap(),
        "--max-concurrency".into(),
        std::env::var_os("LAYA_TEST_CONCURRENCY").unwrap(),
        "--threads".into(),
        "2".into(),
        "--queue-capacity".into(),
        "8".into(),
    ])
    .unwrap()
}

async fn requests(client: Client) {
    let data: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/system-one/mixed-6.json"
    ))
    .unwrap();
    let mut callers = tokio::task::JoinSet::new();
    let barrier = Arc::new(tokio::sync::Barrier::new(9));
    for _ in 0..8 {
        let client = client.clone();
        let barrier = barrier.clone();
        let request = Request::from_slice(
            data["request_json"].as_str().unwrap().as_bytes(),
            &Limits::default(),
        )
        .unwrap();
        callers.spawn(async move {
            barrier.wait().await;
            client.system_one(request).await
        });
    }
    barrier.wait().await;
    let mut count = 0;
    while let Some(result) = callers.join_next().await {
        let response = result.unwrap().unwrap();
        super::parity::check_answers(
            serde_json::to_value(response).unwrap(),
            data["response"].clone(),
            "mixed-6 concurrent",
        );
        count += 1;
    }
    assert_eq!(count, 8);
    client.close();
}

fn check_overlap(runs: &Runs, expected: usize) {
    let events = runs.events.lock().unwrap();
    assert_eq!(events.len(), 16, "exactly one Session::run per request");
    let epoch = events[0].0;
    let mut active = std::collections::BTreeSet::new();
    let mut maximum = 0;
    for (at, id, entering) in events.iter() {
        if *entering {
            assert!(active.insert(*id));
        } else {
            assert!(active.remove(id));
        }
        maximum = maximum.max(active.len());
        assert!(active.len() <= expected);
        println!(
            "run_event: ns={} id={} entering={} active={}",
            at.duration_since(epoch).as_nanos(),
            id,
            entering,
            active.len()
        );
    }
    assert!(active.is_empty());
    assert_eq!(
        maximum, expected,
        "must observe native calls overlapping, not just permits"
    );
    println!(
        "PASS: requests=8 native_runs=8 observed_max={maximum} configured_slots={expected} threads_per_session=2 inter_op=1"
    );
}
