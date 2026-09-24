use super::*;
use crate::scheduler;
use crate::test_http as http;
use std::{
    io::Write,
    net::SocketAddr,
    process::{Child, Command, Stdio},
};
use tokio::time::{sleep, timeout};

const PRIVATE: &str = r#"{"state":"secret-state-17","questions":{"secret-name-17":{"type":"noul","instructions":"secret-instructions-17"}}}"#;

#[test]
#[ignore = "spawned only by the process-level signal test"]
fn process_fixture() {
    if std::env::var_os("LAYA_SIGNAL_FIXTURE").is_none() {
        return;
    }
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .init();
    let mut scheduler = Scheduler::test_worker_with_timeouts(
        1,
        1,
        Duration::from_secs(3),
        if std::env::var_os("LAYA_TIMED_FIXTURE").is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(5)
        },
        |_| {
            let mut byte = [0];
            std::io::Read::read_exact(&mut std::io::stdin(), &mut byte).unwrap();
            Err(scheduler::Error::Inference(crate::engine::Error::Sequence(
                crate::sequence::Error::Tokenizer(
                    std::io::Error::other("secret-state-17 secret-instructions-17 /private/model")
                        .into(),
                ),
            )))
        },
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        println!("ADDRESS={}", listener.local_addr().unwrap());
        serve(
            listener,
            &mut scheduler,
            Limits::default(),
            Duration::from_millis(500),
            crate::api::ApiToken::parse(http::TOKEN).unwrap(),
        )
        .await
        .unwrap();
    });
}

struct Process {
    child: Child,
    address: SocketAddr,
    log: std::path::PathBuf,
}

impl Process {
    async fn start(timed: bool) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let log = std::env::temp_dir().join(format!(
            "laya-signals-{}-{}.log",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut command = Command::new(std::env::current_exe().unwrap());
        if timed {
            command.env("LAYA_TIMED_FIXTURE", "1");
        }
        let child = command
            .args([
                "--exact",
                "server::tests::process_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("LAYA_SIGNAL_FIXTURE", "1")
            .stdin(Stdio::piped())
            .stdout(std::fs::File::create(log.with_extension("stdout")).unwrap())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        // Establish cleanup before the first fallible startup handshake.
        let mut process = Self {
            child,
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            log,
        };
        process.address = timeout(Duration::from_secs(5), process.read_address())
            .await
            .unwrap();
        process
    }

    async fn read_address(&mut self) -> SocketAddr {
        loop {
            let output = std::fs::read_to_string(self.log.with_extension("stdout")).unwrap();
            if let Some(address) = output
                .lines()
                .find_map(|line| line.strip_prefix("ADDRESS="))
            {
                return address.parse().unwrap();
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "fixture exited before ready"
            );
            sleep(Duration::from_millis(5)).await;
        }
    }

    fn signal(&self, signal: &str) {
        assert!(
            Command::new("sh")
                .args([
                    "-c",
                    "kill \"$1\" \"$2\"",
                    "sh",
                    signal,
                    &self.child.id().to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
    }

    async fn exit(&mut self) -> std::process::ExitStatus {
        timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
            self.child.wait().unwrap();
        }
        std::fs::remove_file(&self.log).unwrap();
        std::fs::remove_file(self.log.with_extension("stdout")).unwrap();
    }
}

#[tokio::test]
async fn signals_drain_idle_process_and_allow_restart() {
    for signal in ["-TERM", "-INT"] {
        let mut process = Process::start(false).await;
        assert_eq!(http::get(process.address, "/readyz").await.0, 200);
        process.signal(signal);
        let status = process.exit().await;
        assert_eq!(status.code(), Some(0), "{signal}: {status}");
        let log = std::fs::read_to_string(&process.log).unwrap();
        assert!(log.contains("shutdown_complete"), "{log}");
        assert!(
            tokio::net::TcpStream::connect(process.address)
                .await
                .is_err()
        );
        println!("{signal} idle: ready=200 -> exit=0; listener closed");
    }
}

async fn wait_metric(process: &Process, expected: &str) -> String {
    timeout(Duration::from_secs(3), async {
        loop {
            let (_, _, body) = http::get(process.address, "/metrics").await;
            let text = String::from_utf8(body).unwrap();
            if text.contains(expected) {
                return text;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

async fn stopped_admission(process: &Process) {
    timeout(Duration::from_millis(400), async {
        while http::get(process.address, "/readyz").await.0 != 503 {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        http::receive(http::post(process.address, PRIVATE).await)
            .await
            .0,
        503
    );
}

async fn work_scenario(signal: &str, queued: bool, force: bool, timed: bool) {
    let mut process = Process::start(timed).await;
    assert_eq!(http::get(process.address, "/readyz").await.0, 200);
    let running = http::post(process.address, PRIVATE).await;
    wait_metric(&process, "laya_inference_inflight 1\n").await;
    let waiting = if queued {
        let request = http::post(process.address, PRIVATE).await;
        wait_metric(&process, "laya_queue_size 1\n").await;
        Some(request)
    } else {
        None
    };
    let running = if timed {
        assert_eq!(http::receive(running).await.0, 504);
        let metrics = wait_metric(
            &process,
            "laya_errors_total{reason=\"inference_timeout\"} 1\n",
        )
        .await;
        assert!(metrics.contains("laya_inference_inflight 1\n"));
        None
    } else {
        Some(running)
    };
    process.signal(signal);
    stopped_admission(&process).await;
    if let Some(waiting) = waiting {
        assert_eq!(http::receive(waiting).await.0, 503);
        wait_metric(&process, "laya_queue_size 0\n").await;
    }
    if !force {
        process
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"x")
            .unwrap();
        if let Some(running) = running {
            let (status, _, body) = http::receive(running).await;
            assert_eq!(status, 500);
            assert!(!String::from_utf8(body).unwrap().contains("secret"));
        }
    }
    check_exit(&mut process, force).await;
    println!("{signal} queued={queued} force={force} timed={timed}: ready 200 -> 503, queue -> 0");
}

async fn check_exit(process: &mut Process, force: bool) {
    let status = process.exit().await;
    assert_eq!(status.code(), Some(i32::from(force)));
    let log = std::fs::read_to_string(&process.log).unwrap();
    assert!(!log.contains("secret"), "{log}");
    assert!(!log.contains("/private/model"), "{log}");
    assert!(!log.contains(http::TOKEN));
    let events: Vec<serde_json::Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let event = events
        .iter()
        .find(|v| {
            v["fields"]["event"]
                == if force {
                    "shutdown_grace_expired"
                } else {
                    "shutdown_complete"
                }
        })
        .unwrap();
    assert_eq!(event["fields"]["inflight"], usize::from(force));
    assert_eq!(event["fields"]["tracked_tasks"], usize::from(force));
    assert!(
        tokio::net::TcpStream::connect(process.address)
            .await
            .is_err()
    );
    println!("exit={}; {event}", status.code().unwrap());
}

#[tokio::test]
async fn signals_drain_or_terminate_actual_work_and_clear_queue() {
    for signal in ["-TERM", "-INT"] {
        work_scenario(signal, false, false, false).await;
        work_scenario(signal, true, false, false).await;
        work_scenario(signal, true, true, false).await;
        work_scenario(signal, true, false, true).await;
        work_scenario(signal, true, true, true).await;
    }
}
