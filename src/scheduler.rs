//! Bounded FIFO admission and blocking CPU execution (compatibility.md §7.3).
//!
//! Each loaded Session is an independent slot. Waiter cancellation drops only its
//! queue entry/result receiver; the blocking task owns its Session and permit.
//! Keep the owner and drive `Scheduler::run` alongside requests until `close` and
//! all tasks finish. Cancelling `run(&mut self)` retains its JoinSet: resume it to
//! drain. A shutdown-grace expiry requires process termination, not task abort.
//! No HTTP server, retry, deduplication or cross-request batching lives here.

use std::{
    error::Error as StdError,
    fmt,
    future::poll_fn,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
    time::Duration,
};

use ort::session::Session;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, oneshot},
    task::{JoinError, JoinSet},
    time::{Instant, sleep, timeout_at},
};

use crate::{
    engine,
    postprocess::Calibration,
    sequence::SequenceBuilder,
    system_one::{ErrorBody, ErrorEnvelope, Request, Response},
};

/// Static transport errors; engine sources remain available for internal diagnosis.
#[derive(Debug)]
pub enum Error {
    QueueFull,
    QueueTimeout,
    InferenceTimeout,
    Unavailable,
    Inference(engine::Error),
    TaskFailed,
}

impl Error {
    pub fn status(&self) -> u16 {
        match self {
            Self::QueueFull => 429,
            Self::QueueTimeout | Self::Unavailable => 503,
            Self::InferenceTimeout => 504,
            Self::Inference(error) => error.status(),
            Self::TaskFailed => 500,
        }
    }

    pub fn envelope(&self) -> ErrorEnvelope {
        let (code, message) = match self {
            Self::QueueFull => ("queue_full", "Inference queue full"),
            Self::QueueTimeout => ("queue_timeout", "Inference queue timeout"),
            Self::InferenceTimeout => ("inference_timeout", "Inference wait timeout"),
            Self::Unavailable => ("unavailable", "Service unavailable"),
            Self::Inference(error) => return error.envelope(),
            Self::TaskFailed => ("inference_failed", "Inference failed"),
        };
        ErrorEnvelope {
            error: ErrorBody { code, message },
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.envelope().error.message)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Inference(error) => Some(error),
            _ => None,
        }
    }
}

/// Consistent gauges; `inflight` includes CPU work whose caller has left.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub accepting: bool,
    pub waiting: usize,
    pub inflight: usize,
    pub tracked_tasks: usize,
}

/// Owns task handles; `run` must be driven until closed and drained.
#[must_use = "drive run until close and all blocking tasks have been joined"]
pub struct Scheduler {
    dispatcher: Dispatcher,
    client: Client,
}

/// Cloneable request interface sharing one bounded dispatcher and loaded resources.
#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
    resources: Arc<Resources>,
}

struct Resources {
    sessions: Mutex<Vec<Session>>,
    sequence: SequenceBuilder,
    calibration: Calibration,
}

impl Scheduler {
    /// Takes already-verified sessions; their count is the actual concurrency limit.
    /// # Errors
    /// Rejects empty/oversized slot counts and zero/unrepresentable timeouts.
    pub fn new(
        sessions: Vec<Session>,
        sequence: SequenceBuilder,
        calibration: Calibration,
        queue_capacity: usize,
        queue_timeout: Duration,
        inference_timeout: Duration,
    ) -> Result<Self, &'static str> {
        let dispatcher = Dispatcher::new(
            sessions.len(),
            queue_capacity,
            queue_timeout,
            inference_timeout,
        )?;
        let client = Client {
            shared: dispatcher.shared.clone(),
            resources: Arc::new(Resources {
                sessions: Mutex::new(sessions),
                sequence,
                calibration,
            }),
        };
        Ok(Self { dispatcher, client })
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Reaps every task, including late results, until admission closes and work ends.
    /// # Errors
    /// Returns the first task panic/cancellation after closing admission and draining.
    /// This future is cancel-safe: handles remain in this owner for a later call.
    pub async fn run(&mut self) -> Result<(), JoinError> {
        self.dispatcher.run().await
    }
}

impl Client {
    /// Submit a request already normalized by `Request::from_slice`.
    /// # Errors
    /// Queue/availability/deadline errors or the unchanged engine failure. Dropping
    /// this future cancels waiting admission, never already-started CPU work.
    pub async fn system_one(&self, request: Request) -> Result<Response, Error> {
        let resources = self.resources.clone();
        let shared = self.shared.clone();
        self.shared
            .execute(move || {
                let mut session = lock(&resources.sessions).pop().ok_or_else(|| {
                    shared.close();
                    Error::Unavailable
                })?;
                let result = engine::system_one(
                    &request,
                    &resources.sequence,
                    &mut session,
                    &resources.calibration,
                );
                lock(&resources.sessions).push(session);
                result.map_err(Error::Inference)
            })
            .await
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot()
    }

    /// Stop admission and wake queued requests with unavailable; running work drains.
    pub fn close(&self) {
        self.shared.close();
    }
}

struct Dispatcher {
    shared: Arc<Shared>,
    failure: Option<JoinError>,
}

struct Shared {
    slots: Arc<Semaphore>,
    state: Mutex<State>,
    queue_capacity: usize,
    queue_timeout: Duration,
    inference_timeout: Duration,
}

struct State {
    accepting: bool,
    waiting: usize,
    inflight: usize,
    tasks: JoinSet<bool>,
    driver: Option<Waker>,
}

// Locks protect only short bookkeeping operations, never engine code or an await.
// A poisoned lock implies a programming panic; the task guard closes admission.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Dispatcher {
    fn new(
        slots: usize,
        queue_capacity: usize,
        queue_timeout: Duration,
        inference_timeout: Duration,
    ) -> Result<Self, &'static str> {
        if slots == 0 || slots > Semaphore::MAX_PERMITS {
            return Err("execution slot count is outside the semaphore range");
        }
        for duration in [queue_timeout, inference_timeout] {
            if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
                return Err("scheduler timeout is outside the clock range");
            }
        }
        Ok(Self {
            shared: Arc::new(Shared {
                slots: Arc::new(Semaphore::new(slots)),
                state: Mutex::new(State {
                    accepting: true,
                    waiting: 0,
                    inflight: 0,
                    tasks: JoinSet::new(),
                    driver: None,
                }),
                queue_capacity,
                queue_timeout,
                inference_timeout,
            }),
            failure: None,
        })
    }

    async fn run(&mut self) -> Result<(), JoinError> {
        loop {
            let result = poll_fn(|cx| {
                let mut state = lock(&self.shared.state);
                state.driver = Some(cx.waker().clone());
                match state.tasks.poll_join_next(cx) {
                    Poll::Ready(None) if state.accepting => Poll::Pending,
                    result => result,
                }
            })
            .await;
            match result {
                Some(Ok(true)) => {
                    tracing::warn!(event = "inference_completion", outcome = "failure")
                }
                Some(Ok(false)) => {}
                Some(Err(error)) => {
                    self.shared.close();
                    if self.failure.is_none() {
                        self.failure = Some(error);
                    }
                }
                None => return self.failure.take().map_or(Ok(()), Err),
            }
        }
    }
}

impl Shared {
    fn snapshot(&self) -> Snapshot {
        let state = lock(&self.state);
        Snapshot {
            accepting: state.accepting,
            waiting: state.waiting,
            inflight: state.inflight,
            tracked_tasks: state.tasks.len(),
        }
    }

    fn close(&self) {
        let mut state = lock(&self.state);
        state.accepting = false;
        self.slots.close();
        if let Some(waker) = state.driver.take() {
            waker.wake();
        }
    }

    async fn acquire(self: &Arc<Self>) -> Result<Running, Error> {
        let mut entry = Waiting {
            shared: self.clone(),
            queued: false,
        };
        // A cooperative-budget yield is not a full queue. Only this short permit
        // poll is unconstrained, so Pending means an actual registered waiter.
        let mut acquire = pin!(tokio::task::unconstrained(
            self.slots.clone().acquire_owned()
        ));
        let mut timer = pin!(sleep(Duration::ZERO));
        poll_fn(|cx| {
            let mut state = lock(&self.state);
            if !state.accepting {
                return Poll::Ready(Err(Error::Unavailable));
            }
            if entry.queued && Instant::now() >= timer.deadline() {
                return Poll::Ready(Err(Error::QueueTimeout));
            }
            match acquire.as_mut().poll(cx) {
                Poll::Ready(Ok(permit)) => {
                    Poll::Ready(entry.start(&mut state, permit, timer.deadline()))
                }
                Poll::Ready(Err(_)) => Poll::Ready(Err(Error::Unavailable)),
                Poll::Pending => {
                    if !entry.queued {
                        if state.waiting == self.queue_capacity {
                            return Poll::Ready(Err(Error::QueueFull));
                        }
                        let Some(deadline) = Instant::now().checked_add(self.queue_timeout) else {
                            return Poll::Ready(Err(Error::Unavailable));
                        };
                        timer.as_mut().reset(deadline);
                        state.waiting += 1;
                        entry.queued = true;
                    }
                    timer.as_mut().poll(cx).map(|()| Err(Error::QueueTimeout))
                }
            }
        })
        .await
    }

    async fn execute<R: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce() -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error> {
        let running = self.acquire().await?;
        let deadline = running.deadline;
        let (send, receive) = oneshot::channel();
        let tracing = tracing::dispatcher::get_default(Clone::clone);
        {
            let mut state = lock(&self.state);
            if !state.accepting {
                return Err(Error::Unavailable);
            }
            state.tasks.spawn_blocking(move || {
                let result = tracing::dispatcher::with_default(&tracing, work);
                let failed = result.is_err();
                drop(running);
                if let Err(result) = send.send(result) {
                    // The caller already ended its wait. The owner still observes
                    // completion/failure, including errors buffered before cancel.
                    drop(result);
                }
                failed
            });
            if let Some(waker) = state.driver.take() {
                waker.wake();
            }
        }
        let result = timeout_at(deadline, receive).await;
        if Instant::now() >= deadline {
            return Err(Error::InferenceTimeout);
        }
        match result {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::TaskFailed),
            Err(_) => Err(Error::InferenceTimeout),
        }
    }
}

struct Waiting {
    shared: Arc<Shared>,
    queued: bool,
}

impl Waiting {
    fn start(
        &mut self,
        state: &mut State,
        permit: OwnedSemaphorePermit,
        queue_deadline: Instant,
    ) -> Result<Running, Error> {
        let now = Instant::now();
        if self.queued && now >= queue_deadline {
            return Err(Error::QueueTimeout);
        }
        let deadline = now
            .checked_add(self.shared.inference_timeout)
            .ok_or(Error::Unavailable)?;
        if self.queued {
            state.waiting -= 1;
            self.queued = false;
        }
        state.inflight += 1;
        Ok(Running {
            shared: self.shared.clone(),
            _permit: permit,
            deadline,
        })
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if self.queued {
            lock(&self.shared.state).waiting -= 1;
        }
    }
}

struct Running {
    shared: Arc<Shared>,
    _permit: OwnedSemaphorePermit,
    deadline: Instant,
}

impl Drop for Running {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.shared.close();
        }
        lock(&self.shared.state).inflight -= 1;
    }
}

#[cfg(test)]
mod tests;
