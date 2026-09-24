//! Six metrics from compatibility.md §8. Gauges use one scheduler snapshot at scrape
//! time; counters and histograms are shared per scheduler, never process globals.
use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};
use std::sync::Arc;
use tokio::time::Instant;

use crate::scheduler::{Client, Snapshot};

pub(crate) struct Metrics {
    requests: Counter,
    request_duration: Histogram,
    pub(crate) inference_duration: Histogram,
    errors: Family<Vec<(&'static str, &'static str)>, Counter>,
}

impl Default for Metrics {
    fn default() -> Self {
        let buckets = [
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 30., 60., 120.,
        ];
        Self {
            requests: Counter::default(),
            request_duration: Histogram::new(buckets),
            inference_duration: Histogram::new(buckets),
            errors: Family::default(),
        }
    }
}

impl Metrics {
    pub(crate) fn encode(&self, snapshot: Snapshot) -> Result<String, std::fmt::Error> {
        let mut registry = Registry::default();
        registry.register(
            "laya_requests",
            "System One HTTP requests",
            self.requests.clone(),
        );
        registry.register(
            "laya_request_duration_seconds",
            "HTTP time including queue",
            self.request_duration.clone(),
        );
        registry.register(
            "laya_inference_duration_seconds",
            "Actual Session run time",
            self.inference_duration.clone(),
        );
        registry.register(
            "laya_errors",
            "Failed HTTP requests by terminal reason",
            self.errors.clone(),
        );
        for (name, help, value) in [
            ("laya_queue_size", "Waiting requests", snapshot.waiting),
            (
                "laya_inference_inflight",
                "Occupied execution slots",
                snapshot.inflight,
            ),
        ] {
            let gauge = Gauge::<u64, std::sync::atomic::AtomicU64>::default();
            gauge.set(value as u64);
            registry.register(name, help, gauge);
        }
        let mut text = String::new();
        encode(&mut text, &registry)?;
        Ok(text)
    }
}

/// A dropped HTTP future has a terminal outcome too, but cannot release CPU slots.
pub(crate) struct RequestObservation {
    metrics: Arc<Metrics>,
    client: Client,
    started: Instant,
    pub(crate) outcome: &'static str,
}

impl RequestObservation {
    pub(crate) fn new(client: Client) -> Self {
        let metrics = client.metrics();
        metrics.requests.inc();
        Self {
            metrics,
            client,
            started: Instant::now(),
            outcome: "client_cancelled",
        }
    }
}

impl Drop for RequestObservation {
    fn drop(&mut self) {
        let duration_seconds = self.started.elapsed().as_secs_f64();
        self.metrics.request_duration.observe(duration_seconds);
        if self.outcome != "success" {
            self.metrics
                .errors
                .get_or_create(&vec![("reason", self.outcome)])
                .inc();
        }
        let snapshot = self.client.snapshot();
        tracing::info!(
            event = "request_completion",
            outcome = self.outcome,
            duration_seconds,
            queue = snapshot.waiting,
            inflight = snapshot.inflight
        );
    }
}
