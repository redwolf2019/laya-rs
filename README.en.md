# laya-rs

[简体中文](README.md) | **English**

**Self-host Laya on Linux CPUs. Classify, score, and evaluate propositions through a Rust HTTP service.**

[![Rust application runtime](https://img.shields.io/badge/Rust-runtime-F46623?logo=rust&logoColor=F46623&labelColor=24292F)](#build-and-cli)
[![Native deployment on Linux x86_64 and ARM64](https://img.shields.io/badge/Linux-x86__64%20%7C%20ARM64-FCC624?logo=linux&logoColor=FCC624&labelColor=24292F)](docs/installation.md)
[![Docker supports Linux ARM64 only](https://img.shields.io/badge/Docker-ARM64-2496ED?logo=docker&logoColor=2496ED&labelColor=24292F)](#docker-deployment-linux-arm64-cpu)
[![Original code is MIT licensed](https://img.shields.io/badge/License-MIT-238636?labelColor=24292F)](LICENSE)

`laya-rs` is a Rust inference runtime for Laya System One (System-1) models, served by the `laya-server` executable.
Give it text or structured JSON and a set of typed questions to get choices, scores, or probabilities.
The default model, `laya-multilingual`, supports Chinese, English, and other languages supported by the model.
The model stays in memory, with multiple HTTP clients sharing inference resources.

[Install](#one-command-installation--uninstallation-linux-x86_64-arm64) · [Call the API](#call-the-api) ·
[Compare with JEV](#comparison-with-jev) · [Validation scope](#validation-scope) · [FAQ](#faq)

## What it does

Submit a `state` and a set of `questions`. Answers are returned under their question names:

| Question type | Example use | Result |
| --- | --- | --- |
| `choice` | Which department should handle this ticket? | The selected option and probabilities for all options |
| `score` | How high is the handling priority? | An expected level starting at 0, possibly fractional, with a level distribution |
| `noul` | Does this need human intervention? | The probability that the proposition is true, from 0 to 1 |

Use it for ticket classification, priority scoring, tool routing, or deciding whether to escalate to a person or a larger model.
Answers come from predefined options or levels. The service does not generate chat replies, and a probability is not a guarantee of correctness.

## Comparison with JEV

**JEV (officially styled Jev) is TypeSafe AI's System One model. Laya is a separate System One model project
with publicly available weights. laya-rs provides local Laya inference through a Rust service.**
JEV and Laya both target typed decisions. laya-rs serves Laya locally and does not run JEV weights.
See the [TypeSafe introduction](https://typesafe.ai/blog/introducing-system-one-models-and-jev)
and the [pinned Laya README](https://github.com/he-jev/laya/blob/c5d78730f3493e4fe16d61507ef4b78eef7318cf/README.md) for model descriptions.

| Aspect | JEV / TypeSafe API | laya-rs |
| --- | --- | --- |
| Shared capabilities | Answers Choice, Score, and Noul questions about a state | The same three question types, with typed answers and probabilities |
| Model and deployment | Calls the TypeSafe hosted API using an available model name | Self-hosts a fixed `laya-multilingual` ONNX bundle on Linux CPUs |
| Inference endpoint | `POST /v1/systemone` | `POST /v1/system-one`, with a hyphen |
| Model selection | Requires `model` in the request; available models can be queried with `GET /v1/models` | Loads a fixed model at startup; no model listing or per-request switching |
| Response model identifier | `model` identifies the model that answered | Follows the Laya reference response: `model` is always `"rl-agent"` |
| Authentication and operations | Uses a TypeSafe API key | Uses a self-managed shared Bearer token; includes health checks, metrics, bounded queuing, and graceful shutdown |

Comparison checked on **2026-09-24**: `[Verified/HIGH, limited to public documentation and the repository contract]`.
JEV fields and routes come from the [TypeSafe OpenAPI specification](https://api.typesafe.ai/openapi.json).
laya-rs behavior follows its [API compatibility contract](docs/compatibility.md). This project has not run comparisons against the JEV API.

**laya-rs does not claim to be a drop-in replacement for JEV.** Its compatibility baseline is the in-process
`system_one` semantics of the pinned `he-jev/laya@c5d7873` revision. When migrating a client, check request validation,
response fields, and error handling as well as the URL and token. For example, this project requires `instructions`
for every question and preserves Laya's `rl_agent.act_probability` extension field.
Passing the fixed Laya reference cases does not imply identical answers to JEV. This project has no JEV performance
comparison using the same environment and workload.

Use laya-rs to run a local decision API in your own Linux environment. Use TypeSafe's service if you need the JEV model itself.
For an existing JEV integration, start with your business question definitions, then adapt and validate them against this project's contract.

## Choose a deployment method

| Method | Environment | Prerequisites |
| --- | --- | --- |
| [One-command installation](#one-command-installation--uninstallation-linux-x86_64-arm64) | Linux x86_64 / ARM64 host or virtual machine | Interactive terminal, root or sudo; downloads the program, runtime libraries, and model automatically |
| [Docker deployment](#docker-deployment-linux-arm64-cpu) | Linux ARM64 container | Docker and the fixed model bundle; build the image locally |
| [Build from source](#build-and-cli) | Linux development environment | Rust toolchain, fixed model bundle, and ONNX Runtime CPU shared library |

Running the service requires no Python, Node.js, PyTorch, or GPU. ONNX Runtime is a native dependency;
the Rust implementation refers to the application code. Exporting the model yourself requires a separate Python/PyTorch preparation environment.

## One-command installation / uninstallation (Linux x86_64, ARM64)

Run this on a Linux host or virtual machine with an interactive terminal and root or sudo access.
The installer downloads and verifies the program, runtime libraries, and fixed model, then configures a background service
and startup on boot. Press Enter to use the defaults, or enter `e` to edit the configuration.
**The default bind address is `0.0.0.0:8080`. API access requires a token; the installer does not change firewall rules.**

Install or upgrade (downloads the complete script before executing it; interactive input is read from the terminal):

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" install )
```

Uninstall:

```sh
( script=$(mktemp) || exit; trap 'rm -f "$script"' EXIT; curl -fsSL --proto '=https' --tlsv1.2 https://github.com/redwolf2019/laya-rs/releases/latest/download/install.sh -o "$script" && sh "$script" uninstall )
```

Uninstallation preserves the model and configuration by default. A full removal requires selecting that option and typing `DELETE`.
Upgrades preserve configuration and tokens, reuse valid model files, and roll back to the previous version if acceptance checks fail.
A successful installation must pass authenticated, real inference checks for all three question types in Chinese.

The token is stored in `/etc/laya-server/token.env`, readable only by root. Read it with
`sudo cat /etc/laya-server/token.env`. Check readiness with `curl -fsS http://127.0.0.1:8080/readyz`.
See the **[native installation guide](docs/installation.md)** for configuration, authenticated requests, service management, upgrades, and removal.
The installer supports systemd, OpenRC, runit, and dinit, and explicitly rejects unknown environments.
See [installer validation](docs/validation/installer.md) for the tested distributions, architectures, and service managers.

## Call the API

This example assumes `LAYA_API_TOKEN` is configured on the client. If you used the installer, follow
[checks and API calls](docs/installation.md#检查与调用) to read the installed configuration and send an authenticated request.
Once the service is running, send a request containing all three question types:

```sh
curl -fsS http://127.0.0.1:8080/v1/system-one \
  -H "Authorization: Bearer $LAYA_API_TOKEN" \
  -H 'Content-Type: application/json' \
  --data '{
    "state": "The customer reports a duplicate charge and wants an immediate refund.",
    "questions": {
      "department": {
        "type": "choice",
        "instructions": "Which department should handle this request?",
        "criteria": {"billing": "Payments, refunds, and invoices", "technical": "Technical issues", "sales": "Sales inquiries"}
      },
      "priority": {
        "type": "score",
        "instructions": "What is the handling priority of this request?",
        "criteria": ["Low", "Medium", "High", "Urgent"]
      },
      "urgent": {
        "type": "noul",
        "instructions": "Is this request urgent?"
      }
    }
  }'
```

A successful response contains `model`, `answers`, and `usage`. For this example:

- `answers.department.choice` is the selected department key; `probabilities` contains the probability for each department.
- `answers.priority.score` is a score from 0 to 3, corresponding to the ordered levels from “Low” to “Urgent.”
- `answers.urgent.noul` is the probability that the request is urgent.

`usage.input_tokens` is the input token count; `usage.output_tokens` is 0.
See the [API contract](docs/compatibility.md) for complete response fields, input limits, and error codes.

| Route | Purpose |
| --- | --- |
| `POST /v1/system-one` | Requires a Bearer token; submits state and questions, returning inference results |
| `GET /healthz` | No authentication; checks whether the process is alive |
| `GET /readyz` | No authentication; checks whether the service can accept inference requests |
| `GET /metrics` | Requires a Bearer token; returns Prometheus/OpenMetrics metrics |

A missing, incorrect, or duplicate Authorization header returns `401` with
`{"error":{"code":"unauthorized","message":"Unauthorized"}}`.
Prometheus scraping also requires Bearer credentials. To check manually:

```sh
curl -fsS http://127.0.0.1:8080/metrics -H "Authorization: Bearer $LAYA_API_TOKEN"
```

## Docker deployment (Linux ARM64 CPU)

You need Docker and the fixed model bundle. The current Dockerfile supports only `linux/arm64`.
The configuration below uses the validated allocation of 8 vCPUs and 12 GiB of memory; reserve those resources in Docker Desktop.
See the [MVP validation report](docs/mvp-validation.md) for the test environment and performance data.

### 1. Prepare the model

Download the fixed `model-<id>.tar.gz` from [GitHub Releases](https://github.com/redwolf2019/laya-rs/releases)
and extract it into `models/multilingual/`. The native installer handles this automatically.
To export it yourself, follow the [model preparation guide](tools/model-prep/README.md) for the one-time export and reference comparison.
Only the export requires a separate Python/PyTorch environment; building the Docker image and running the service do not.

Prepare these files under the repository root. Their contents must match the [fixed model manifest](docs/model-manifest.json):

```text
models/multilingual/
├── laya.onnx
├── laya.onnx.data
├── laya_config.json
├── tokenizer/
│   ├── tokenizer.json
│   └── tokenizer_config.json
└── licenses/                 # All license files listed in the manifest
```

Startup checks file sizes and SHA-256 hashes. Other model or configuration versions are not accepted.
The container user `65532:65532` must be able to read these files. Keep the model directory unchanged while the service is running.

### 2. Build and start

Run from the repository root. The build downloads dependencies but does not download the model:

```sh
export LAYA_API_TOKEN="$(openssl rand -hex 32)"
docker build --platform linux/arm64 -t laya-rs:local .
docker run -d --name laya-server --platform linux/arm64 \
  --env LAYA_API_TOKEN \
  --cpus 8 --memory 12g --memory-swap 12g \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  --mount "type=bind,source=$PWD/models/multilingual,target=/models/multilingual,readonly" \
  -p 127.0.0.1:8080:8080 laya-rs:local \
  --model /models/multilingual --threads 1 --inter-op-threads 1 \
  --max-concurrency 1 --shutdown-grace 120
```

Read the startup logs and check readiness:

```sh
docker logs laya-server
curl -fsS http://127.0.0.1:8080/readyz
```

The port does not listen until model verification and warmup finish. Once ready, the endpoint returns `{"status":"ready"}`.
If the container exits, check the logs, model files, read permissions, and memory allocation first.

Allow time for in-flight requests when stopping the service:

```sh
docker stop --timeout 130 laya-server
docker rm laya-server
```

`--timeout` should exceed the service's `--shutdown-grace` value, which defaults to 120 seconds.

`LAYA_API_TOKEN` is a shared secret for all trusted clients. Distribute it to backend services or internal scripts through your deployment's secret management.
The service refuses to start if the token is missing, empty, or malformed. Tokens do not expire automatically.
After changing the token, recreate the container with the new environment variable, or restart the process if running the binary directly.
Do not store tokens in source code, logs, or browser frontends.
Use a gateway for production HTTPS, and restrict the service's HTTP port to that gateway or a trusted internal network.

## Build and CLI

To run from source, install Rustup and C/C++ build tools (`gcc g++ libc6-dev pkg-config`) on Linux,
and prepare the model bundle and ONNX Runtime CPU 1.28.0 shared library.
The repository pins Rust 1.98.1. See the [runtime environment guide](docs/validation-environment.md#官方-cpu-原生库)
for obtaining and verifying the native library.

```sh
cargo build --release --locked
./target/release/laya-server --help
# Configure LAYA_API_TOKEN first; for a local trial, generate it with openssl rand -hex 32 and export it.
./target/release/laya-server --model ./models/multilingual \
  --ort-library /opt/onnxruntime/lib/libonnxruntime.so --threads 2 --max-concurrency 1
```

Replace `--ort-library` with the actual path to your shared library.
See `--help` or the [CLI contract](docs/compatibility.md#71-cli-与限制) for all options and defaults.

## Validation scope

Repository validation records as of **2026-09-24**, `[Verified/HIGH, limited to the listed models, inputs, and environments]`:

- **Laya semantic comparison**: 21 fixed requests passed real inference comparisons on Linux ARM64 CPUs.
  Fields rounded to four decimal places and discrete answers matched exactly; logits and unrounded probabilities met the contract tolerances.
  See [postprocessing validation](docs/validation/postprocess.md) and [HTTP validation](docs/validation/http.md).
- **Load and lifecycle**: 2,180 requests across 12 load configurations on Docker Desktop Linux ARM64 passed comparisons against fixed Laya answers,
  along with resource, signal, and shutdown regression checks. See the [MVP validation report](docs/mvp-validation.md)
  for the environment, raw data, and reproduction commands. Latency and throughput do not generalize to other hardware or inputs.
- **Native release packages**: release CI on both x86_64 and ARM64 passed authenticated, real inference for all three question types in Chinese.
  See [installer validation](docs/validation/installer.md) for the separate coverage of systemd VM lifecycles,
  other service managers, and public installation commands.

These records verify consistency with a fixed reference. They are not business classification accuracy evaluations,
evaluations of every supported language, or certification of JEV client compatibility.

## FAQ

### Can it run offline? Is business data sent to JEV?

Once the program, runtime libraries, and fixed model are available, laya-rs runs inference locally without calling JEV or other cloud inference APIs.
Installation downloads and source builds need access to dependencies; offline operation is different from a first-time offline installation.
The service does not log state or question bodies by default. See [observability and shutdown validation](docs/validation/observability.md)
for logging and metrics boundaries.

### How much text can it process?

The fixed model has a **1024-token** sequence budget per question, including the question, options, and state.
It is not a separate 1024-token budget for the input text. Long states are truncated to the remaining budget,
and each question in a request gets its own sequence. See [sequences and batching](docs/compatibility.md#3-序列与-batch) for the exact rules.

### How fast is inference?

Latency depends on the CPU, thread count, number of questions, sequence length, and queuing. Measure capacity using your own workload.
This project's Linux CPU measurements are in the [benchmark results](docs/mvp-validation.md#实测结果).
Upstream GPU measurements and JEV hosted API measurements are not performance promises for this service.

## Documentation

The detailed guides linked below are currently in Chinese.

| Task | Documentation |
| --- | --- |
| Install, upgrade, manage, or uninstall the service | [Native installation guide](docs/installation.md) |
| Integrate clients and handle limits or errors | [API / CLI compatibility contract](docs/compatibility.md) |
| Check model provenance and file integrity | [Fixed model manifest](docs/model-manifest.json) · [Model preparation guide](tools/model-prep/README.md) |
| Understand the inference pipeline and implementation boundaries | [Service plan](docs/laya-server-plan.md) · [Domain terminology](CONTEXT.md) |
| Review functionality and deployment evidence | [MVP validation](docs/mvp-validation.md) · [Installer validation](docs/validation/installer.md) |

## License

Original project code is licensed under the [MIT License](LICENSE). See [NOTICE](NOTICE.md) for attribution of ported code.
Model weights and third-party runtime libraries are governed by their respective licenses.
