use super::*;
use std::{future::Future, pin::pin, sync::mpsc, task::Poll};

async fn pending<F: Future>(future: std::pin::Pin<&mut F>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn exhausted_async_budget_does_not_turn_an_idle_slot_into_queue_full() {
    let mut owner = Dispatcher::new(1, 0, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    for _ in 0..128 {
        tokio::task::consume_budget().await;
    }
    assert_eq!(dispatch.execute(|| Ok(1)).await.unwrap(), 1);
    dispatch.close();
    owner.run().await.unwrap();
}

#[tokio::test]
async fn cancelled_running_waiter_holds_slot_until_cpu_finishes() {
    let mut owner =
        Dispatcher::new(2, 1, Duration::from_secs(30), Duration::from_secs(120)).unwrap();
    let dispatch = owner.shared.clone();
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let mut gates = Vec::new();
    let mut waiters = Vec::new();
    for id in 0..2 {
        let (release, gate) = mpsc::channel();
        gates.push(release);
        let started = started.clone();
        let dispatch = dispatch.clone();
        waiters.push(tokio::spawn(async move {
            dispatch
                .execute(move || {
                    started.send(id).unwrap();
                    gate.recv().unwrap();
                    Ok(id)
                })
                .await
        }));
    }
    starts.recv().await.unwrap();
    starts.recv().await.unwrap();
    waiters[0].abort();
    assert!(waiters.remove(0).await.unwrap_err().is_cancelled());
    let third = dispatch.execute(move || {
        started.send(2).unwrap();
        Ok(2)
    });
    let mut third = pin!(third);
    pending(third.as_mut()).await;
    assert_eq!(dispatch.snapshot().waiting, 1);
    assert_eq!(dispatch.snapshot().inflight, 2);
    assert!(matches!(
        dispatch.execute(|| Ok(3)).await,
        Err(Error::QueueFull)
    ));
    assert!(starts.try_recv().is_err());
    gates[0].send(()).unwrap();
    assert_eq!(third.await.unwrap(), 2);
    assert_eq!(starts.recv().await.unwrap(), 2);
    gates[1].send(()).unwrap();
    assert_eq!(waiters.remove(0).await.unwrap().unwrap(), 1);
    dispatch.close();
    owner.run().await.unwrap();
    assert_eq!(dispatch.snapshot().waiting, 0);
    assert_eq!(dispatch.snapshot().inflight, 0);
    assert_eq!(dispatch.snapshot().tracked_tasks, 0);
}

#[tokio::test]
async fn queued_cancellation_restores_capacity_and_preserves_fifo() {
    let mut owner =
        Dispatcher::new(1, 2, Duration::from_secs(30), Duration::from_secs(120)).unwrap();
    let dispatch = owner.shared.clone();
    let (release, gate) = mpsc::channel();
    let (started, start) = oneshot::channel();
    let first = dispatch.execute(move || {
        started.send(()).unwrap();
        gate.recv().unwrap();
        Ok(0)
    });
    let mut first = pin!(first);
    pending(first.as_mut()).await;
    start.await.unwrap();
    let mut cancelled = Box::pin(
        dispatch.execute(|| -> Result<(), Error> { panic!("cancelled queue entry executed") }),
    );
    pending(cancelled.as_mut()).await;
    let (order, mut calls) = tokio::sync::mpsc::unbounded_channel();
    let (release_next, next_gate) = mpsc::channel();
    let next_order = order.clone();
    let second = dispatch.execute(move || {
        next_order.send(1).unwrap();
        next_gate.recv().unwrap();
        Ok(1)
    });
    let mut second = pin!(second);
    pending(second.as_mut()).await;
    drop(cancelled);
    assert_eq!(dispatch.snapshot().waiting, 1);
    let third = dispatch.execute(move || {
        order.send(2).unwrap();
        Ok(2)
    });
    let mut third = pin!(third);
    pending(third.as_mut()).await;
    release.send(()).unwrap();
    first.await.unwrap();
    pending(third.as_mut()).await;
    pending(second.as_mut()).await;
    assert_eq!(calls.recv().await.unwrap(), 1);
    assert!(calls.try_recv().is_err());
    release_next.send(()).unwrap();
    assert_eq!(second.await.unwrap(), 1);
    assert_eq!(third.await.unwrap(), 2);
    assert_eq!(calls.recv().await.unwrap(), 2);
    dispatch.close();
    owner.run().await.unwrap();
    assert_eq!(dispatch.snapshot().waiting, 0);
}

#[tokio::test(start_paused = true)]
async fn queue_deadline_wins_over_a_simultaneously_available_slot() {
    let mut owner =
        Dispatcher::new(1, 1, Duration::from_secs(3), Duration::from_secs(120)).unwrap();
    let dispatch = owner.shared.clone();
    let (release, gate) = mpsc::channel();
    let (started, start) = oneshot::channel();
    let first = dispatch.execute(move || {
        started.send(()).unwrap();
        gate.recv().unwrap();
        Ok(0)
    });
    let mut first = pin!(first);
    pending(first.as_mut()).await;
    start.await.unwrap();
    let queued = dispatch.execute(|| -> Result<(), Error> { panic!("expired work executed") });
    let mut queued = pin!(queued);
    pending(queued.as_mut()).await;
    tokio::time::advance(Duration::from_secs(3)).await;
    release.send(()).unwrap();
    first.await.unwrap();
    assert!(matches!(queued.await, Err(Error::QueueTimeout)));
    assert_eq!(dispatch.snapshot().waiting, 0);
    assert_eq!(dispatch.execute(|| Ok(1)).await.unwrap(), 1);
    dispatch.close();
    owner.run().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn inference_timeout_keeps_slot_and_late_failure_is_reaped() {
    let mut owner = Dispatcher::new(1, 0, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    let (release, gate) = mpsc::channel();
    let (started, start) = oneshot::channel();
    let first = dispatch.execute(move || -> Result<(), Error> {
        started.send(()).unwrap();
        gate.recv().unwrap();
        Err(Error::Inference(engine::Error::MissingOutput))
    });
    let mut first = pin!(first);
    pending(first.as_mut()).await;
    start.await.unwrap();
    tokio::time::advance(Duration::from_secs(5)).await;
    assert!(matches!(first.await, Err(Error::InferenceTimeout)));
    assert_eq!(dispatch.snapshot().inflight, 1);
    assert!(matches!(
        dispatch.execute(|| Ok(0)).await,
        Err(Error::QueueFull)
    ));
    dispatch.close();
    release.send(()).unwrap();
    owner.run().await.unwrap();
    assert_eq!(dispatch.snapshot().inflight, 0);
    assert_eq!(dispatch.snapshot().tracked_tasks, 0);
}

#[tokio::test]
async fn engine_failure_reuses_slot_and_panic_closes_admission_then_joins() {
    let mut owner = Dispatcher::new(1, 1, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    assert!(matches!(
        dispatch
            .execute(|| -> Result<(), Error> {
                Err(Error::Inference(engine::Error::MissingOutput))
            })
            .await,
        Err(Error::Inference(engine::Error::MissingOutput))
    ));
    assert_eq!(dispatch.execute(|| Ok(1)).await.unwrap(), 1);
    assert!(matches!(
        dispatch
            .execute(|| -> Result<(), Error> { panic!("test worker panic") })
            .await,
        Err(Error::TaskFailed)
    ));
    assert!(!dispatch.snapshot().accepting);
    assert!(matches!(
        dispatch.execute(|| Ok(1)).await,
        Err(Error::Unavailable)
    ));
    assert!(owner.run().await.unwrap_err().is_panic());
    assert_eq!(dispatch.snapshot().inflight, 0);
    assert_eq!(dispatch.snapshot().tracked_tasks, 0);
}

#[tokio::test(start_paused = true)]
async fn shutdown_rejects_queue_and_cancelled_driver_can_resume_draining() {
    let mut owner = Dispatcher::new(1, 1, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    let (release, gate) = mpsc::channel();
    let (started, start) = oneshot::channel();
    let first = dispatch.execute(move || {
        started.send(()).unwrap();
        gate.recv().unwrap();
        Ok(0)
    });
    let mut first = pin!(first);
    pending(first.as_mut()).await;
    start.await.unwrap();
    let queued = dispatch.execute(|| -> Result<(), Error> { panic!("shutdown queue ran") });
    let mut queued = pin!(queued);
    pending(queued.as_mut()).await;
    dispatch.close();
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(matches!(queued.await, Err(Error::Unavailable)));
    assert_eq!(dispatch.snapshot().waiting, 0);
    {
        let mut driver = Box::pin(owner.run());
        pending(driver.as_mut()).await;
        drop(driver);
    }
    assert_eq!(dispatch.snapshot().inflight, 1);
    assert_eq!(dispatch.snapshot().tracked_tasks, 1);
    release.send(()).unwrap();
    first.await.unwrap();
    owner.run().await.unwrap();
    assert_eq!(dispatch.snapshot().tracked_tasks, 0);
}

#[tokio::test(start_paused = true)]
async fn result_at_execution_deadline_is_timeout_even_if_already_ready() {
    let mut owner = Dispatcher::new(1, 0, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    let response = dispatch.execute(|| Ok(7));
    let mut response = pin!(response);
    pending(response.as_mut()).await;
    dispatch.close();
    owner.run().await.unwrap();
    tokio::time::advance(Duration::from_secs(5)).await;
    assert!(matches!(response.await, Err(Error::InferenceTimeout)));
    assert_eq!(dispatch.snapshot().inflight, 0);
}

#[test]
fn scheduler_errors_use_frozen_status_and_static_envelope() {
    for (error, status, code, message) in [
        (Error::QueueFull, 429, "queue_full", "Inference queue full"),
        (
            Error::QueueTimeout,
            503,
            "queue_timeout",
            "Inference queue timeout",
        ),
        (
            Error::InferenceTimeout,
            504,
            "inference_timeout",
            "Inference wait timeout",
        ),
        (
            Error::Unavailable,
            503,
            "unavailable",
            "Service unavailable",
        ),
        (
            Error::TaskFailed,
            500,
            "inference_failed",
            "Inference failed",
        ),
    ] {
        assert_eq!(error.status(), status);
        assert_eq!(error.envelope().error.code, code);
        assert_eq!(error.to_string(), message);
    }
    assert!(Dispatcher::new(0, 0, Duration::from_secs(1), Duration::from_secs(1)).is_err());
    assert!(
        Dispatcher::new(
            Semaphore::MAX_PERMITS + 1,
            0,
            Duration::from_secs(1),
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert!(Dispatcher::new(1, 0, Duration::ZERO, Duration::from_secs(1)).is_err());
}

struct FailureEvents(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl tracing::Subscriber for FailureEvents {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "laya_server::scheduler"
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
    fn event(&self, _: &tracing::Event<'_>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn buffered_failure_is_recorded_when_caller_later_times_out() {
    let events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _subscriber = tracing::subscriber::set_default(FailureEvents(events.clone()));
    let mut owner = Dispatcher::new(1, 0, Duration::from_secs(3), Duration::from_secs(5)).unwrap();
    let dispatch = owner.shared.clone();
    let (release, gate) = mpsc::channel();
    let (started, start) = oneshot::channel();
    let response = dispatch.execute(move || -> Result<(), Error> {
        started.send(()).unwrap();
        gate.recv().unwrap();
        Err(Error::Inference(engine::Error::MissingOutput))
    });
    let mut response = pin!(response);
    pending(response.as_mut()).await;
    start.await.unwrap();
    tokio::time::advance(Duration::from_secs(5)).await;
    dispatch.close();
    release.send(()).unwrap();
    owner.run().await.unwrap();
    assert!(matches!(response.await, Err(Error::InferenceTimeout)));
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 1);
}
