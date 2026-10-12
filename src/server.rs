//! HTTP and scheduler lifetime: platform stop notifications close admission, drain actual work,
//! then close connections. Grace expiry terminates the process with unfinished counts.
use crate::{scheduler::Scheduler, system_one::Limits};
use std::{error::Error, fmt, io, time::Duration};

#[derive(Debug)]
pub enum ServiceError {
    Configuration(&'static str),
    Io(&'static str, io::Error),
    Task(tokio::task::JoinError),
    Stopped,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Configuration(message) | Self::Io(message, _) => message,
            Self::Task(_) => "blocking task failed",
            Self::Stopped => "inference scheduler stopped",
        })
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(_, error) => Some(error),
            Self::Task(error) => Some(error),
            Self::Configuration(_) | Self::Stopped => None,
        }
    }
}

/// Drive HTTP and every blocking task until a platform stop notification or resource failure.
/// # Errors
/// Listener/signal I/O failures or failed scheduler tasks. Grace expiry exits 1.
pub async fn serve(
    listener: tokio::net::TcpListener,
    scheduler: &mut Scheduler,
    limits: Limits,
    grace: Duration,
    api_token: crate::api::ApiToken,
) -> Result<(), ServiceError> {
    use std::future::IntoFuture;
    let client = scheduler.client();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let mut http = std::pin::pin!(
        axum::serve(
            listener,
            crate::api::router(client.clone(), limits, api_token)
        )
        .with_graceful_shutdown(async {
            if stopped.await.is_err() {
                tracing::warn!(event = "http_stop_sender_closed");
            }
        })
        .into_future()
    );
    tracing::info!(event = "http_started");
    let mut http_done = false;
    let result = tokio::select! {
        result = stop_notification() => result,
        result = &mut http => {
            http_done = true;
            result.map_err(|e| ServiceError::Io("HTTP server failed", e)).and(Err(ServiceError::Stopped))
        }
        result = scheduler.run() => result.map_err(ServiceError::Task).and(Err(ServiceError::Stopped)),
    };
    client.close();
    let snapshot = client.snapshot();
    tracing::info!(
        event = "shutdown_started",
        queue = snapshot.waiting,
        inflight = snapshot.inflight
    );
    drain(scheduler, &client, http, http_done, stop, grace).await?;
    result
}

#[cfg(unix)]
async fn stop_notification() -> Result<(), ServiceError> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate())
        .map_err(|e| ServiceError::Io("cannot register SIGTERM", e))?;
    let mut interrupt = signal(SignalKind::interrupt())
        .map_err(|e| ServiceError::Io("cannot register SIGINT", e))?;
    tokio::select! {
        _ = terminate.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    }
}

#[cfg(windows)]
async fn stop_notification() -> Result<(), ServiceError> {
    let mut interrupt = tokio::signal::windows::ctrl_c()
        .map_err(|e| ServiceError::Io("cannot register Ctrl+C", e))?;
    let mut terminate = tokio::signal::windows::ctrl_break()
        .map_err(|e| ServiceError::Io("cannot register Ctrl+Break", e))?;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    // A launcher can pass down the ignore-Ctrl+C flag. The null handler only
    // clears that flag; Tokio's registered callback continues to own delivery.
    // SAFETY: null/false is the documented toggle and borrows no Rust memory.
    if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
        return Err(ServiceError::Io(
            "cannot enable Ctrl+C",
            io::Error::last_os_error(),
        ));
    }
    tokio::select! {
        _ = interrupt.recv() => Ok(()),
        _ = terminate.recv() => Ok(()),
        result = stop_file() => result,
    }
}

#[cfg(windows)]
async fn stop_file() -> Result<(), ServiceError> {
    let Some(path) = std::env::var_os("LAYA_STOP_FILE") else {
        return std::future::pending().await;
    };
    loop {
        match std::fs::metadata(&path) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ServiceError::Io(
                    "cannot read local stop notification",
                    error,
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn drain(
    scheduler: &mut Scheduler,
    client: &crate::scheduler::Client,
    http: impl std::future::Future<Output = io::Result<()>>,
    http_done: bool,
    stop: tokio::sync::oneshot::Sender<()>,
    grace: Duration,
) -> Result<(), ServiceError> {
    let drain = async {
        let (owner, http) = tokio::join!(
            async {
                let result = scheduler.run().await.map_err(ServiceError::Task);
                if stop.send(()).is_err() {
                    tracing::warn!(event = "http_already_stopped");
                }
                result
            },
            async {
                if http_done {
                    Ok(())
                } else {
                    http.await
                        .map_err(|e| ServiceError::Io("HTTP server failed", e))
                }
            }
        );
        owner.and(http)
    };
    match tokio::time::timeout(grace, drain).await {
        Ok(drained) => {
            drained?;
            tracing::info!(
                event = "shutdown_complete",
                inflight = client.snapshot().inflight,
                tracked_tasks = client.snapshot().tracked_tasks
            );
            Ok(())
        }
        Err(_) => force_exit(client),
    }
}

fn force_exit(client: &crate::scheduler::Client) -> ! {
    let snapshot = client.snapshot();
    tracing::error!(
        event = "shutdown_grace_expired",
        queue = snapshot.waiting,
        inflight = snapshot.inflight,
        tracked_tasks = snapshot.tracked_tasks
    );
    // Runtime drop waits indefinitely for spawn_blocking; only process exit can
    // enforce the frozen deadline without claiming that CPU tasks were cancelled.
    std::process::exit(1)
}

#[cfg(test)]
mod tests;
