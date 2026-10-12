# laya-rs

[简体中文](README.md) | **English**

**Self-host Laya on Linux, Windows, or macOS CPUs. Classify, score, and evaluate propositions through a Rust HTTP service.**

[![Rust application runtime](https://img.shields.io/badge/Rust-runtime-F46623?logo=rust&logoColor=F46623&labelColor=24292F)](#build-and-cli)
[![Native deployment on Linux x86_64 and ARM64](https://img.shields.io/badge/Linux-x86__64%20%7C%20ARM64-FCC624?logo=linux&logoColor=FCC624&labelColor=24292F)](docs/installation.md)
[![Windows 11 x64 local candidate](https://img.shields.io/badge/Windows_11-x64_candidate-0078D4)](#windows-11-x64-portable-build)
[![macOS ARM64 local candidate](https://img.shields.io/badge/macOS-ARM64_candidate-555555?logo=apple)](#macos-apple-silicon-portable-build)
[![Docker supports Linux ARM64 only](https://img.shields.io/badge/Docker-ARM64-2496ED?logo=docker&logoColor=2496ED&labelColor=24292F)](#docker-deployment-linux-arm64-cpu)
[![Original code is MIT licensed](https://img.shields.io/badge/License-MIT-238636?labelColor=24292F)](LICENSE)

`laya-rs` is a Rust inference runtime for Laya System One (System-1) models, served by the `laya-server` executable.
Give it text or structured JSON and a set of typed questions to get choices, scores, or probabilities.
The default model, `laya-multilingual`, supports Chinese, English, and other languages supported by the model.
The model stays in memory, with multiple HTTP clients sharing inference resources.

[Choose a platform](#choose-a-deployment-method) · [Linux](#one-command-installation--uninstallation-linux-x86_64-arm64) ·
[Windows](#windows-11-x64-portable-build) · [macOS](#macos-apple-silicon-portable-build) · [Call the API](#call-the-api) ·
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
| Model and deployment | Calls the TypeSafe hosted API using an available model name | Self-hosts a fixed `laya-multilingual` ONNX bundle on Linux, Windows x64, or macOS ARM64 CPUs; release and validation limits are listed below |
| Inference endpoint | `POST /v1/systemone` | `POST /v1/system-one`, with a hyphen |
| Model selection | Requires `model` in the request; available models can be queried with `GET /v1/models` | Loads a fixed model at startup; no model listing or per-request switching |
| Response model identifier | `model` identifies the model that answered | Follows the Laya reference response: `model` is always `"rl-agent"` |
| Authentication and operations | Uses a TypeSafe API key | Uses a self-managed shared Bearer token; includes health checks, metrics, bounded queuing, and graceful shutdown |

External JEV sources were checked on **2026-09-24**; this project's platform information was checked on **2026-10-12**:
`[Verified/HIGH, limited to public documentation and the repository contract]`.
JEV fields and routes come from the [TypeSafe OpenAPI specification](https://api.typesafe.ai/openapi.json).
laya-rs behavior follows its [API compatibility contract](docs/compatibility.md). This project has not run comparisons against the JEV API.

**laya-rs does not claim to be a drop-in replacement for JEV.** Its compatibility baseline is the in-process
`system_one` semantics of the pinned `he-jev/laya@c5d7873` revision. When migrating a client, check request validation,
response fields, and error handling as well as the URL and token. For example, this project requires `instructions`
for every question and preserves Laya's `rl_agent.act_probability` extension field.
Passing the fixed Laya reference cases does not imply identical answers to JEV. This project has no JEV performance
comparison using the same environment and workload.

Use laya-rs to run a local decision API on the platforms listed below. Use TypeSafe's service if you need the JEV model itself.
For an existing JEV integration, start with your business question definitions, then adapt and validate them against this project's contract.

## Choose a deployment method

Checked on **2026-10-12**. The public stable program release, [v0.1.1](https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.1),
contains Linux assets only. Windows and macOS portable builds are implemented, but their candidates are local and have no public download yet.

| Platform and minimum version | CPU architecture | Deployment and entry point | Actual validation coverage |
| --- | --- | --- | --- |
| Linux; no single distribution/kernel minimum defined | x86_64 / ARM64 | [Installer and system service](#one-command-installation--uninstallation-linux-x86_64-arm64); interactive terminal, curl, root/sudo | Native release CI; systemd lifecycle on Debian 12 VMs; partial Alpine/musl and other service-manager checks in [installer validation](docs/validation/installer.md) |
| Windows 11 | x64 | [PowerShell 5.1 console portable build](#windows-11-x64-portable-build); requires VC++ x64 runtime | 11 Pro for Workstations build 26300; standard-user, clean-system/VC-UAC and public distribution checks pending; does not extend to Windows 10, Server, x86, or ARM64 |
| macOS 14+ (deployment target) | Apple Silicon ARM64 | [Shell foreground portable build](#macos-apple-silicon-portable-build); system tools; not notarized | Native 27.0.1 / M2 Max; 14, clean-system and manual Gatekeeper approval checks pending; no Intel, Universal, or Rosetta support |
| Docker: Linux containers | ARM64 | [Build the Docker image locally](#docker-deployment-linux-arm64-cpu) | Docker Desktop Linux ARM64; this Dockerfile does not support linux/amd64 or Windows/macOS containers |

[Source builds](#build-and-cli) are also available for the native platforms. Detailed guides:
[Linux](docs/installation.md), [Windows](docs/windows.md), [macOS](docs/macos.md), currently in Chinese.
The Windows/macOS scripts do not register system services or startup-on-boot tasks.

Running a precompiled package requires no Rust, Xcode, Python, Node.js, PyTorch, or GPU. Native runtime libraries
ship with the program; Windows also checks the system VC++ runtime. ONNX Runtime is a native dependency;
the Rust implementation refers to the application code. Exporting the model yourself requires a separate Python/PyTorch preparation environment.

The program and [fixed `model-3bff67d421ae27d4.tar.gz` bundle](https://github.com/redwolf2019/laya-rs/releases/tag/model-3bff67d421ae27d4)
are distributed separately. Installation checks sizes, SHA-256 hashes, and the model's per-file manifest; a valid copy of the same model is reused.
Checksums from the same source check integrity; they are not independent digital signatures.

### Linux starting configuration (running the service only)

These values are for the precompiled service and fixed `laya-multilingual` model, excluding source builds and model exports:

| Resource | Recommendation |
| --- | --- |
| CPU | 4 cores / 4 vCPUs |
| Memory | 6 GiB |
| Disk | At least 5 GiB free, excluding the OS; for the program, model, downloads, extraction, and logs |
| GPU / VRAM | Not required |

This is a starting recommendation based on [installer validation](docs/validation/installer.md),
[model size](docs/model-manifest.json), and [performance records](docs/mvp-validation.md).
Strict hardware minimums have not been measured, and the Linux configuration is not a validated Windows/macOS minimum.
The macOS installer separately requires download size plus 8 GiB free on the staging volume and at least 4 GiB on the program volume.
See the platform guides for dependencies and space checks. Reserve additional resources for long inputs, more questions, higher concurrency, and retained logs.

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

Programs live in `/opt/laya-server`, models in `/var/lib/laya-server/models`, and configuration in `/etc/laya-server`.
The token is in root-owned `/etc/laya-server/token.env` with mode 0600; the service runs as a dedicated non-root user.
After installation, check readiness and use the stored token for the package's real three-type request (POSIX shell):

```sh
curl -fsS http://127.0.0.1:8080/readyz
sudo sh -c 'token=$(sed -n "s/^LAYA_API_TOKEN=//p" /etc/laya-server/token.env); printf "Authorization: Bearer %s\n" "$token" | curl -fsS --header @- -H "Content-Type: application/json" --data-binary @/opt/laya-server/current/smoke.json http://127.0.0.1:8080/v1/system-one'
```

For a normal systemd stop, use `sudo systemctl stop laya-server`; see the guide below for other service managers and their lifecycle integration.
See the **[native installation guide](docs/installation.md)** for configuration, authenticated requests, service management, upgrades, and removal.
The installer supports systemd, OpenRC, runit, and dinit, and explicitly rejects unknown environments.
See [installer validation](docs/validation/installer.md) for the tested distributions, architectures, and service managers.

## Windows 11 x64 portable build

For now, [build and assemble a local candidate](docs/windows.md#操作与恢复) on a development machine,
or use the same candidate directory supplied by a maintainer after checking its hashes. `v0.1.1` has no Windows package.
The directory must contain `laya.ps1` (from `scripts/laya.ps1`), `laya-server-windows-x86_64.zip`,
`release.env`, and the fixed `model-3bff67d421ae27d4.tar.gz` archive.
The tested candidate is `v0.1.2-candidate`; see [Windows validation](docs/validation/windows.md) for its hashes and coverage.

Open **Windows PowerShell 5.1** in that candidate directory and verify the script's source before running it.
Follow the [detailed guide](docs/windows.md#操作与恢复) for download marking and organization execution policies.
Running the package requires no Rust/MSVC development tools. If VC++ is absent or too old, the script asks for consent,
verifies the Microsoft installer's signature, and requests UAC for the runtime installation only.
Cancellation, failure, or a required reboot is not reported as success.

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Install -ReleaseDirectory (Get-Location).Path
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Start
Invoke-RestMethod 'http://127.0.0.1:8080/readyz'
$root = Join-Path $env:LOCALAPPDATA 'laya-server'
$headers = @{ Authorization = 'Bearer '+[IO.File]::ReadAllText((Join-Path $root 'token')) }
$json = '{"state":"urgent refund request","questions":{"q":{"type":"noul","instructions":"Is this urgent?"}}}'
Invoke-RestMethod 'http://127.0.0.1:8080/v1/system-one' -Method Post -Headers $headers -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($json))
Remove-Variable headers
```

`RemoteSigned` applies to the child process only; organization policy still takes precedence.
The example uses `Invoke-RestMethod`, avoiding the PowerShell `curl` alias.
Install runs real inference checks and leaves a first deployment stopped; Start launches a separate console process.
Stop it from another window if needed:

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\laya.ps1 Stop
```

The data root is `%LOCALAPPDATA%\laya-server`: `programs` holds programs, `models` holds models,
`current.json`/`previous.json` hold configuration, and `token` holds the secret.
Directory and token ACLs restrict access to the current user and SYSTEM; actual access denial for another standard user remains untested.
Use `Update -ReleaseDirectory (Get-Location).Path` to read a new candidate placed in that directory.
Update without a local directory becomes usable once Windows assets are published.
See [operations and recovery](docs/windows.md#操作与恢复) for failed-update rollback, `Update -Recover`, external models, and configuration options.

## macOS Apple Silicon portable build

For now, [build a local candidate](docs/macos.md#开发与发布) and generate `release.env` using the macOS
`Real deployment, update and rollback` step in the [release workflow](.github/workflows/release.yml), or use the same candidate directory supplied by a maintainer
after checking its hashes. `v0.1.1` has no macOS package.
The directory must contain `laya-macos.sh` (from `scripts/laya-macos.sh`), `laya-server-macos-aarch64.tar.gz`,
`release.env`, and the fixed `model-3bff67d421ae27d4.tar.gz` archive.
The tested candidate is `v0.1.1-macos21`; hashes are in [macOS validation](docs/validation/macos.md).

The executable and dylib have **ad-hoc signatures and are not notarized**. Browser downloads may carry quarantine attributes;
verify the source and follow the [manual approval guidance for the specific file](docs/macos.md#未公证分发).
That Gatekeeper flow has not been tested. The script neither disables protection nor removes quarantine automatically.
Use the system Bash and tools as a normal user; sudo, Homebrew, and Xcode are not required to run it.

From a terminal in the candidate directory (callable from Bash or zsh):

```sh
bash ./laya-macos.sh install --release-directory "$PWD"
bash ./laya-macos.sh start
```

install only prepares files. start validates the deployment, runs real three-type inference, and stays in the current terminal.
Open another terminal to check readiness and call the API:

```sh
curl -fsS http://127.0.0.1:8080/readyz
LAYA_API_TOKEN=$(cat "$HOME/Library/Application Support/laya-server/token")
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | \
  curl -fsS --header @- -H 'Content-Type: application/json' \
  --data '{"state":"urgent refund request","questions":{"q":{"type":"noul","instructions":"Is this urgent?"}}}' \
  http://127.0.0.1:8080/v1/system-one
unset LAYA_API_TOKEN
```

Return to the candidate directory to stop it:

```sh
bash ./laya-macos.sh stop
```

The data root is `$HOME/Library/Application Support/laya-server`: directories use 0700, the `token` file uses 0600,
and inherited ACLs are removed. `programs`, `models`, `current.env`/`previous.env`, and `run` hold programs,
models, configuration, and logs respectively. Use `update --release-directory "$PWD"` to read a new candidate placed in that directory;
update without it becomes usable once macOS assets are published. A previously running instance stays attached to the update terminal after an update or rollback.
See [updates and recovery](docs/macos.md#升级回退和中断恢复) for `update --recover` and external-directory rules.

### Network, tokens, and portable-build maintenance

All three native installers default to `0.0.0.0:8080`, threads=2, inter-op=1, max-concurrency=1,
and shutdown-grace=120 seconds. The general CLI defaults to threads=8 and max-concurrency=2; the other defaults listed here are the same.
For local-only access, pass `-Listen '127.0.0.1:8080'` to Windows Install or `--listen 127.0.0.1:8080`
to macOS install; on Linux, edit the interactive configuration. Adjust request URLs if you change the port.
The scripts leave the firewall unchanged. Restrict HTTP to a trusted network and use a gateway for production HTTPS.

Initial preparation generates a shared Bearer token; startup does not print it, and upgrades and rollbacks reuse it.
Do not enable shell tracing, print authorization headers, or put the token in source code, logs, URLs, or browser frontends.
To rotate it, stop the service, replace the token file contents with a new cryptographically random token
(keep the `LAYA_API_TOKEN=` prefix on Linux), preserve ownership and permissions, then start the service and update clients.
There is no online rotation or automatic expiry.

Windows Ctrl+C/Ctrl+Break, macOS Ctrl+C, and the scripts' Stop/stop enter normal drain.
Draining within grace exits with 0; expiry exits nonzero. Closing a window, logging out, shutting down, or force-killing can interrupt inference or updates.
Version switching has downtime: download/verification failures preserve the old process; a new version that fails real inference checks triggers rollback and revalidation.
Configuration, tokens, valid models, and old programs are retained. Neither portable script has an uninstall command:
stop and confirm process exit, then remove that deployment's directories manually. Back up tokens, configuration, and models you want to keep;
external model directories are separate from deployment cleanup.

## Call the API

This is a POSIX shell example and assumes `LAYA_API_TOKEN` is configured on the client; the Windows quick start above uses PowerShell.
If you used the Linux installer, follow
[checks and API calls](docs/installation.md#检查与调用) to read the installed configuration and send an authenticated request.
Once the service is running, send a request containing all three question types:

```sh
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | \
  curl -fsS http://127.0.0.1:8080/v1/system-one --header @- \
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
printf 'Authorization: Bearer %s\n' "$LAYA_API_TOKEN" | curl -fsS http://127.0.0.1:8080/metrics --header @-
```

## Docker deployment (Linux ARM64 CPU)

You need Docker and the fixed model bundle. The current Dockerfile supports only `linux/arm64`.
The configuration below uses the validated allocation of 8 vCPUs and 12 GiB of memory; reserve those resources in Docker Desktop.
See the [MVP validation report](docs/mvp-validation.md) for the test environment and performance data.

### 1. Prepare the model

Download the fixed [`model-3bff67d421ae27d4.tar.gz`](https://github.com/redwolf2019/laya-rs/releases/download/model-3bff67d421ae27d4/model-3bff67d421ae27d4.tar.gz) from [GitHub Releases](https://github.com/redwolf2019/laya-rs/releases)
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

This section is for developers; precompiled packages do not require build tools.
The repository pins Rust 1.98.1, `ort 2.0.0-rc.13`, ORT CPU 1.28.0, and tokenizers 0.23.2 using Cargo.lock.

| Build platform / Rust target | Build tools and ORT |
| --- | --- |
| Linux / `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | Rustup, C/C++ (`gcc g++ libc6-dev pkg-config`); `libonnxruntime.so`, see [native-library setup and verification](docs/validation-environment.md#官方-cpu-原生库) |
| Windows / `x86_64-pc-windows-msvc` | Rust, MSVC C/C++, and Windows SDK; build in x64 Developer PowerShell. Loads `onnxruntime.dll` beside the exe by default; see [Windows build instructions](docs/windows.md#操作与恢复) |
| macOS / `aarch64-apple-darwin` | Rust, Xcode/CLT and SDK, `MACOSX_DEPLOYMENT_TARGET=14.0`; loads `libonnxruntime.dylib` beside the executable by default. This project builds CPU-only ORT separately because non-CPU devices in the official macOS package are rejected. See [macOS build instructions](docs/macos.md#开发与发布) for CMake/Python/Git dependency-preparation tools |

Linux source-run example:

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

Platform and release status checked on **2026-10-12**. The existing validation records below are
`[Verified/HIGH, limited to the listed models, inputs, and environments]`. CI without models does not establish real inference or performance results.

- **Laya semantic comparison**: 21 fixed requests passed real inference comparisons on Linux ARM64 CPUs.
  Fields rounded to four decimal places and discrete answers matched exactly; logits and unrounded probabilities met the contract tolerances.
  See [postprocessing validation](docs/validation/postprocess.md) and [HTTP validation](docs/validation/http.md).
- **Load and lifecycle**: 2,180 requests across 12 load configurations on Docker Desktop Linux ARM64 passed comparisons against fixed Laya answers,
  along with resource, signal, and shutdown regression checks. See the [MVP validation report](docs/mvp-validation.md)
  for the environment, raw data, and reproduction commands. Latency and throughput do not generalize to other hardware or inputs.
- **Native release packages**: release CI on both x86_64 and ARM64 passed authenticated, real inference for all three question types in Chinese.
  See [installer validation](docs/validation/installer.md) for the separate coverage of systemd VM lifecycles,
  other service managers, and public installation commands.
- **Windows local candidate**: #20 implementation `5403ddd`, independent probe `fdc7123`, and scope closure `90fd6e4`.
  `v0.1.2-candidate` passed 21 requests, HTTP, shutdown, updates, and rollback on Windows 11 build 26300 / i5-13600KF.
  Standard-user ACL, clean-system/VC-UAC, and public candidate checks remain pending; see [Windows records](docs/validation/windows.md).
- **macOS local candidate**: #21 implementation `121a103`.
  `v0.1.1-macos21` passed 21 requests, HTTP, shutdown, updates, and rollback on macOS 27.0.1 / M2 Max.
  macOS 14, operation without development tools, browser quarantine/manual Gatekeeper approval, and formal distribution remain untested;
  see [macOS records](docs/validation/macos.md).

#20/#21 were closed under user-adjusted scopes; closure does not mark deferred checks as passed.
The historical Linux validation date remains 2026-09-24; the platform candidates were validated on 2026-10-12.
See the [README verification record](docs/validation/readme.md) for this pass over commands, download entry points, and links.

These records verify consistency with a fixed reference. They are not business classification accuracy evaluations,
evaluations of every supported language, or certification of JEV client compatibility.

## FAQ

### Can Windows/macOS install directly from latest?

Not yet: public `v0.1.1` contains Linux programs only. Use the local-candidate paths above.
Building a candidate requires a development environment; running a prepared candidate does not.
Windows 10 and macOS Intel/Rosetta are outside this scope, and macOS 14 is a deployment target only.
Adding native platforms does not change Docker's Linux ARM64 restriction.

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
Windows/macOS resource observations are in their respective validation records.
Linux, upstream GPU, and hosted JEV measurements are not performance promises for other platforms or production workloads.

## Documentation

The detailed guides linked below are currently in Chinese.

| Task | Documentation |
| --- | --- |
| Install, upgrade, manage, or uninstall the Linux service | [Native installation guide](docs/installation.md) |
| Run the Windows console build, prepare dependencies, upgrade, or recover | [Windows guide](docs/windows.md) · [Validation](docs/validation/windows.md) |
| Run the macOS foreground build, check notarization limits, or recover | [macOS guide](docs/macos.md) · [Validation](docs/validation/macos.md) |
| Integrate clients and handle limits or errors | [API / CLI compatibility contract](docs/compatibility.md) |
| Check model provenance and file integrity | [Fixed model manifest](docs/model-manifest.json) · [Model preparation guide](tools/model-prep/README.md) |
| Understand the inference pipeline and implementation boundaries | [Service plan](docs/laya-server-plan.md) · [Domain terminology](CONTEXT.md) |
| Review functionality and deployment evidence | [MVP validation](docs/mvp-validation.md) · [Installer validation](docs/validation/installer.md) |

## License

Original project code is licensed under the [MIT License](LICENSE). See [NOTICE](NOTICE.md) for attribution of ported code.
Model weights and third-party runtime libraries are governed by their respective licenses.
