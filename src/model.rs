//! Startup-only loading of the manifest-pinned #7/#13 bundle. The embedded manifest is trusted;
//! files must remain immutable after verification (mount the bundle read-only).
//! Graph identity pins its independently inspected external-data references, so
//! validating every listed file also validates their location and contents.

use std::{error::Error as StdError, fmt, fs::File, io::Read, path::Path};

use crate::config::Config;
use laya_server::{postprocess::Calibration, sequence::SequenceBuilder};
use ort::{
    session::Session,
    value::{Tensor, TensorElementType, ValueType},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
pub(crate) struct ModelConfig {
    pub max_len: usize,
    pub head_max_len: usize,
    #[serde(flatten)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "validated at startup; execution entry point is tracked in #14"
        )
    )]
    pub calibration: Calibration,
}

impl ModelConfig {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let config: Self = serde_json::from_slice(bytes)
            .map_err(|e| Error::caused("invalid model JSON configuration", e))?;
        let expected = manifest()?.config;
        if config.max_len != expected.max_len || config.head_max_len != expected.head_max_len {
            return Err(Error::new("model token budgets differ from manifest"));
        }
        Ok(config)
    }
}

#[derive(Debug)]
pub(crate) struct Error {
    message: &'static str,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl Error {
    fn new(message: &'static str) -> Self {
        Self {
            message,
            source: None,
        }
    }

    fn caused(message: &'static str, source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self {
            message,
            source: Some(source.into()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<BundleFile>,
    config: ModelConfig,
    session: Vec<TensorSpec>,
    special_tokens: std::collections::BTreeMap<String, u32>,
}

#[derive(Deserialize)]
struct TensorSpec {
    name: String,
    dtype: String,
    shape: Vec<serde_json::Value>,
}

/// Resources are constructed once before listening and owned until shutdown.
pub(crate) struct Model {
    pub sessions: Vec<Session>,
    pub sequence: SequenceBuilder,
    pub config: ModelConfig,
}

impl Model {
    /// Fails without returning partially initialized resources. This process must
    /// not use ort before this call; the native environment is process-global.
    pub fn load(config: &Config) -> Result<Self, Error> {
        verify_bundle(&config.model)?;
        let settings = ModelConfig::from_slice(&read(&config.model.join("laya_config.json"))?)?;
        let sequence = load_sequence(&config.model, &settings)?;
        initialize_runtime(&config.ort_library)?;
        let mut sessions = Vec::new();
        for _ in 0..config.max_concurrency {
            let mut session = create_session(config)?;
            validate_interface(session.inputs().iter().map(|v| (v.name(), v.dtype())), true)?;
            validate_interface(
                session.outputs().iter().map(|v| (v.name(), v.dtype())),
                false,
            )?;
            smoke(&mut session)?;
            sessions.push(session);
        }
        Ok(Self {
            sessions,
            sequence,
            config: settings,
        })
    }
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|e| Error::caused("bundle file is missing or unreadable", e))
}

fn load_sequence(directory: &Path, settings: &ModelConfig) -> Result<SequenceBuilder, Error> {
    let tokenizer = Tokenizer::from_bytes(read(&directory.join("tokenizer/tokenizer.json"))?)
        .map_err(|e| Error::caused("tokenizer JSON is invalid", e))?;
    let bytes = read(&directory.join("tokenizer/tokenizer_config.json"))?;
    let sequence = SequenceBuilder::new(tokenizer, &bytes, settings.max_len, settings.head_max_len)
        .map_err(|e| Error::caused("sequence configuration is invalid", e))?;
    let config: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::caused("tokenizer configuration is invalid", e))?;
    for (key, expected) in manifest()?.special_tokens {
        let token = key
            .strip_suffix("_id")
            .and_then(|key| config.get(key))
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::new("special token configuration is invalid"))?;
        if sequence.tokenizer().token_to_id(token) != Some(expected) {
            return Err(Error::new("special token ID differs from manifest"));
        }
    }
    Ok(sequence)
}

fn initialize_runtime(path: &Path) -> Result<(), Error> {
    let environment = ort::init_from(path)
        .map_err(|e| Error::caused("native ONNX Runtime load or ABI check failed", e))?;
    if !environment.with_telemetry(false).commit() {
        return Err(Error::new("ONNX Runtime was already initialized"));
    }
    let environment = ort::environment::Environment::current()
        .map_err(|e| Error::caused("ONNX Runtime environment creation failed", e))?;
    let mut cpu = false;
    for device in environment.devices() {
        let provider = device
            .ep()
            .map_err(|e| Error::caused("cannot inspect CPU provider", e))?;
        if provider != "CPUExecutionProvider" {
            return Err(Error::new("native runtime exposes a non-CPU provider"));
        }
        cpu = true;
    }
    if !cpu {
        return Err(Error::new("CPU provider is unavailable"));
    }
    Ok(())
}

fn create_session(config: &Config) -> Result<Session, Error> {
    let build = || -> ort::Result<Session> {
        Session::builder()?
            .with_no_environment_execution_providers()?
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])?
            .with_intra_threads(config.threads)?
            .with_inter_threads(config.inter_op_threads)?
            .with_parallel_execution(config.inter_op_threads > 1)?
            .commit_from_file(config.model.join("laya.onnx"))
    };
    build().map_err(|e| Error::caused("CPU Session creation failed", e))
}

fn validate_interface<'a>(
    actual: impl Iterator<Item = (&'a str, &'a ValueType)>,
    inputs: bool,
) -> Result<(), Error> {
    let manifest = manifest()?;
    let expected = if inputs {
        &manifest.session[..5]
    } else {
        &manifest.session[5..]
    };
    let actual: Vec<_> = actual.collect();
    if actual.len() != expected.len() {
        return Err(Error::new("Session tensor count differs from manifest"));
    }
    for ((name, dtype), expected) in actual.into_iter().zip(expected) {
        validate_tensor(name, dtype, expected)?;
    }
    Ok(())
}

/// #7 `tensor-L2` probe (verify.py): a fixed tensor test, not a Sequence Builder.
fn smoke(session: &mut Session) -> Result<[[f32; 2]; 2], Error> {
    let mut run = || -> ort::Result<[[f32; 2]; 2]> {
        let outputs = session.run(ort::inputs! {
            "input_ids" => Tensor::from_array(([1, 2], vec![2_i64, 1]))?,
            "attention_mask" => Tensor::from_array(([1, 2], vec![1_i64, 1]))?,
            "marker_pos" => Tensor::from_array(([1, 2], vec![0_i64, 1]))?,
            "marker_mask" => Tensor::from_array(([1, 2], vec![true, true]))?,
            "qtype" => Tensor::from_array(([1], vec![0_i64]))?,
        })?;
        let mut result = [[0.0; 2]; 2];
        for (index, name) in ["logits", "act_probs"].into_iter().enumerate() {
            let output = outputs
                .get(name)
                .ok_or_else(|| ort::Error::new("missing smoke output"))?;
            let (shape, values) = output.try_extract_tensor::<f32>()?;
            if shape.as_ref() != [1, 2] || values.iter().any(|value| !value.is_finite()) {
                return Err(ort::Error::new("invalid smoke output shape or values"));
            }
            if name == "act_probs"
                && (values.iter().any(|v| !(0.0..=1.0).contains(v))
                    || (values.iter().sum::<f32>() - 1.0).abs() > 0.00011)
            {
                return Err(ort::Error::new("invalid smoke probabilities"));
            }
            result[index].copy_from_slice(values);
        }
        Ok(result)
    };
    run().map_err(|e| Error::caused("real CPU startup inference failed", e))
}

#[derive(Deserialize)]
struct BundleFile {
    path: String,
    bytes: u64,
    sha256: String,
}

fn manifest() -> Result<Manifest, Error> {
    serde_json::from_str(include_str!("../docs/model-manifest.json"))
        .map_err(|e| Error::caused("embedded manifest is invalid", e))
}

pub(crate) fn verify_bundle(directory: &Path) -> Result<(), Error> {
    for entry in manifest()?.files {
        let mut file = File::open(directory.join(entry.path))
            .map_err(|e| Error::caused("bundle file is missing or unreadable", e))?;
        let metadata = file
            .metadata()
            .map_err(|e| Error::caused("bundle file metadata is unreadable", e))?;
        if !metadata.is_file() || metadata.len() != entry.bytes {
            return Err(Error::new("bundle file size differs from manifest"));
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|e| Error::caused("bundle file is unreadable", e))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if digest != entry.sha256 {
            return Err(Error::new("bundle file SHA-256 differs from manifest"));
        }
    }
    Ok(())
}

fn validate_tensor(name: &str, dtype: &ValueType, expected: &TensorSpec) -> Result<(), Error> {
    let ValueType::Tensor {
        ty,
        shape,
        dimension_symbols,
    } = dtype
    else {
        return Err(Error::new("Session value is not a tensor"));
    };
    let dtype = match ty {
        TensorElementType::Int64 => "tensor(int64)",
        TensorElementType::Bool => "tensor(bool)",
        TensorElementType::Float32 => "tensor(float)",
        _ => "unsupported",
    };
    if name != expected.name || dtype != expected.dtype || shape.len() != expected.shape.len() {
        return Err(Error::new(
            "Session tensor name, dtype or rank differs from manifest",
        ));
    }
    for (i, dim) in expected.shape.iter().enumerate() {
        let matches = match dim {
            serde_json::Value::String(symbol) => shape[i] == -1 && dimension_symbols[i] == *symbol,
            serde_json::Value::Number(n) => {
                n.as_i64() == Some(shape[i]) && dimension_symbols[i].is_empty()
            }
            _ => false,
        };
        if !matches {
            return Err(Error::new("Session tensor dimension differs from manifest"));
        }
    }
    Ok(())
}
