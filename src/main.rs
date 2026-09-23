//! CLI entry point. Validate and warm CPU resources before the future HTTP server.

use std::process::ExitCode;

mod config;
mod model;

const HELP: &str = "laya-server — local CPU model loading; HTTP is not implemented

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
Normal startup always exits nonzero until the service is implemented.";

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
    eprintln!("Service not implemented: HTTP serving is unavailable");
    ExitCode::FAILURE
}
