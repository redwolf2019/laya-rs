//! Validate and warm all CPU resources before listening; drive the task owner.

use std::{error::Error, fmt, io, process::ExitCode};

mod config;
mod model;

const HELP: &str = "laya-server — local CPU System One HTTP server

Usage: laya-server --model DIR [OPTIONS]
Pass each option and its value as separate arguments.

  --model DIR               Required local multilingual bundle directory
  --ort-library FILE        Local ORT 1.28 CPU library [default: libonnxruntime.so]
  --listen IP:PORT          Listen address [default: 0.0.0.0:8080]; nonzero port
  --threads N               ORT intra-op threads [default: 8]; 1..=2147483647
  --inter-op-threads N      ORT inter-op threads [default: 1]; 1..=2147483647
  --max-concurrency N       Actual execution slots [default: 2]; positive
  --max-body-bytes N        Request body bytes [default: 1048576]; positive
  --max-questions N         Questions per request [default: 16]; positive
  --max-options N           Choice/Score options [default: 32]; at least 2
  --max-json-depth N        JSON depth, root = 1 [default: 64]; positive
  --queue-capacity N        Waiting requests [default: 32]; zero disables queue
  --queue-timeout SECONDS   Queue wait [default: 30]; positive integer seconds
  --inference-timeout SECONDS  Execution wait [default: 120]; positive seconds
  --shutdown-grace SECONDS  Shutdown deadline [default: 120]; positive seconds
  -h, --help                Print help without loading a model

Integers must fit the platform; timeout deadlines must be representable.
All model resources are validated before the HTTP listener starts.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let config = match config::Config::from_args(args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Invalid configuration: {error}");
            return ExitCode::from(2);
        }
    };
    let model = match model::Model::load(&config) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("Model initialization failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "CPU model initialized: {} sessions, {} vocabulary entries, max_len={}",
        model.sessions.len(),
        model.sequence.tokenizer().get_vocab_size(true),
        model.config.max_len
    );
    if let Err(error) = serve(model, &config) {
        eprintln!("Service failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[derive(Debug)]
enum ServiceError {
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

fn serve(model: model::Model, config: &config::Config) -> Result<(), ServiceError> {
    let mut scheduler = laya_server::scheduler::Scheduler::new(
        model.sessions,
        model.sequence,
        model.config.calibration,
        config.queue_capacity,
        config.queue_timeout,
        config.inference_timeout,
    )
    .map_err(ServiceError::Configuration)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| ServiceError::Io("cannot create async runtime", error))?;
    runtime.block_on(serve_http(&mut scheduler, config))
}

async fn serve_http(
    scheduler: &mut laya_server::scheduler::Scheduler,
    config: &config::Config,
) -> Result<(), ServiceError> {
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|error| ServiceError::Io("cannot bind HTTP listener", error))?;
    let client = scheduler.client();
    let app = laya_server::api::router(
        client.clone(),
        laya_server::system_one::Limits {
            max_body_bytes: config.max_body_bytes,
            max_questions: config.max_questions,
            max_options: config.max_options,
            max_json_depth: config.max_json_depth,
        },
    );
    eprintln!("HTTP listener started");
    tokio::select! {
        result = axum::serve(listener, app) => {
            client.close();
            scheduler.run().await.map_err(ServiceError::Task)?;
            result.map_err(|error| ServiceError::Io("HTTP server failed", error))
        }
        result = scheduler.run() => {
            result.map_err(ServiceError::Task)?;
            Err(ServiceError::Stopped)
        }
    }
}
