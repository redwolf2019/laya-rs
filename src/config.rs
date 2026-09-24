//! The CLI is the only configuration source (docs/compatibility.md §7.1).
//! Values are validated without echoing input. Only directory existence is checked;
//! bundle contents and resource compatibility are checked by the startup loader.

use std::{
    ffi::{OsStr, OsString},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::{Duration, Instant},
};

use laya_server::system_one::Limits;

#[derive(Debug)]
pub(crate) struct Config {
    pub model: PathBuf,
    pub ort_library: PathBuf,
    pub listen: SocketAddr,
    pub threads: usize,
    pub inter_op_threads: usize,
    pub max_concurrency: usize,
    pub max_body_bytes: usize,
    pub max_questions: usize,
    pub max_options: usize,
    pub max_json_depth: usize,
    pub queue_capacity: usize,
    pub queue_timeout: Duration,
    pub inference_timeout: Duration,
    pub shutdown_grace: Duration,
}

impl Config {
    /// Parse option/value pairs without the executable name; errors contain only static text.
    pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Result<Self, &'static str> {
        let limits = Limits::default();
        let mut config = Self {
            model: PathBuf::new(),
            ort_library: PathBuf::from("libonnxruntime.so"),
            listen: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8080)),
            threads: 8,
            inter_op_threads: 1,
            max_concurrency: 2,
            max_body_bytes: limits.max_body_bytes,
            max_questions: limits.max_questions,
            max_options: limits.max_options,
            max_json_depth: limits.max_json_depth,
            queue_capacity: 32,
            queue_timeout: Duration::from_secs(30),
            inference_timeout: Duration::from_secs(120),
            shutdown_grace: Duration::from_secs(120),
        };
        let mut args = args.into_iter();
        while let Some(option) = args.next() {
            let value = args
                .next()
                .ok_or("expected an option and a value; use --help")?;
            config.set(&option, &value)?;
        }
        if !config.model.is_dir() {
            return Err("--model must name an existing local directory");
        }
        Ok(config)
    }

    fn set(&mut self, option: &OsStr, value: &OsStr) -> Result<(), &'static str> {
        match option.to_str() {
            Some("--model") => self.model = PathBuf::from(value),
            Some("--ort-library") => self.ort_library = PathBuf::from(value),
            Some("--listen") => {
                self.listen = value
                    .to_str()
                    .and_then(|s| s.parse().ok())
                    .filter(|addr: &SocketAddr| addr.port() != 0)
                    .ok_or("--listen requires an IP address and a nonzero port")?;
            }
            Some("--threads") => self.threads = threads(value)?,
            Some("--inter-op-threads") => self.inter_op_threads = threads(value)?,
            Some("--max-concurrency") => {
                let count = integer(value, 1)?;
                if count > tokio::sync::Semaphore::MAX_PERMITS {
                    return Err("concurrency exceeds the semaphore range");
                }
                self.max_concurrency = count;
            }
            Some("--max-body-bytes") => self.max_body_bytes = integer(value, 1)?,
            Some("--max-questions") => self.max_questions = integer(value, 1)?,
            Some("--max-options") => self.max_options = integer(value, 2)?,
            Some("--max-json-depth") => self.max_json_depth = integer(value, 1)?,
            Some("--queue-capacity") => self.queue_capacity = integer(value, 0)?,
            Some("--queue-timeout") => self.queue_timeout = timeout(value)?,
            Some("--inference-timeout") => self.inference_timeout = timeout(value)?,
            Some("--shutdown-grace") => self.shutdown_grace = timeout(value)?,
            _ => return Err("unknown option; use --help"),
        }
        Ok(())
    }
}

fn integer(value: &OsStr, minimum: usize) -> Result<usize, &'static str> {
    value
        .to_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse().ok())
        .filter(|n| *n >= minimum)
        .ok_or("integer option is outside its documented range; use --help")
}

fn threads(value: &OsStr) -> Result<usize, &'static str> {
    let count = integer(value, 1)?;
    // ORT 1.28 SetIntraOpNumThreads / SetInterOpNumThreads accept C int.
    i32::try_from(count).map_err(|_| "thread count exceeds the ORT C int range")?;
    Ok(count)
}

fn timeout(value: &OsStr) -> Result<Duration, &'static str> {
    let seconds =
        u64::try_from(integer(value, 1)?).map_err(|_| "timeout seconds exceed the clock range")?;
    let duration = Duration::from_secs(seconds);
    Instant::now()
        .checked_add(duration)
        .ok_or("timeout deadline exceeds the clock range")?;
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Config {
        Config::from_args(args.iter().map(OsString::from)).unwrap()
    }

    #[test]
    fn defaults_match_the_frozen_cli_contract() {
        let config = parse(&["--model", "."]);
        assert_eq!(config.model, PathBuf::from("."));
        assert_eq!(config.ort_library, PathBuf::from("libonnxruntime.so"));
        assert_eq!(config.listen.to_string(), "0.0.0.0:8080");
        assert_eq!(config.threads, 8);
        assert_eq!(config.inter_op_threads, 1);
        assert_eq!(config.max_concurrency, 2);
        assert_eq!(config.max_body_bytes, 1_048_576);
        assert_eq!(config.max_questions, 16);
        assert_eq!(config.max_options, 32);
        assert_eq!(config.max_json_depth, 64);
        assert_eq!(config.queue_capacity, 32);
        assert_eq!(config.queue_timeout, Duration::from_secs(30));
        assert_eq!(config.inference_timeout, Duration::from_secs(120));
        assert_eq!(config.shutdown_grace, Duration::from_secs(120));
    }

    #[test]
    fn cli_overrides_every_default_at_the_lower_bound() {
        let config = parse(&[
            "--model",
            "src",
            "--ort-library",
            "/local/libonnxruntime.so",
            "--listen",
            "[::1]:1",
            "--threads",
            "1",
            "--inter-op-threads",
            "1",
            "--max-concurrency",
            "1",
            "--max-body-bytes",
            "1",
            "--max-questions",
            "1",
            "--max-options",
            "2",
            "--max-json-depth",
            "1",
            "--queue-capacity",
            "0",
            "--queue-timeout",
            "1",
            "--inference-timeout",
            "1",
            "--shutdown-grace",
            "1",
        ]);
        assert_eq!(config.model, PathBuf::from("src"));
        assert_eq!(
            config.ort_library,
            PathBuf::from("/local/libonnxruntime.so")
        );
        assert_eq!(config.listen.to_string(), "[::1]:1");
        assert_eq!(config.threads, 1);
        assert_eq!(config.inter_op_threads, 1);
        assert_eq!(config.max_concurrency, 1);
        assert_eq!(config.max_body_bytes, 1);
        assert_eq!(config.max_questions, 1);
        assert_eq!(config.max_options, 2);
        assert_eq!(config.max_json_depth, 1);
        assert_eq!(config.queue_capacity, 0);
        assert_eq!(config.queue_timeout, Duration::from_secs(1));
        assert_eq!(config.inference_timeout, Duration::from_secs(1));
        assert_eq!(config.shutdown_grace, Duration::from_secs(1));
    }

    #[test]
    fn native_thread_limit_is_accepted_without_truncation() {
        let config = parse(&["--model", ".", "--threads", "2147483647"]);
        assert_eq!(config.threads, 2_147_483_647);
        assert_eq!(
            parse(&["--model", ".", "--inter-op-threads", "2"]).inter_op_threads,
            2
        );
    }

    #[test]
    fn concurrency_cannot_exceed_the_runtime_semaphore_range() {
        let value = (tokio::sync::Semaphore::MAX_PERMITS + 1).to_string();
        assert!(
            Config::from_args(["--model", ".", "--max-concurrency", &value].map(OsString::from))
                .is_err()
        );
    }
}
