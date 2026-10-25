// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    env, fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use clap::{Args, Subcommand};
use flate2::{Compression, write::ZlibEncoder};
use serde_json::{Value, json};
use tokio::{
    fs as tokio_fs,
    io::{AsyncBufReadExt, BufReader},
    runtime::Builder,
};

const ANTHROPIC_COUNT_TOKENS_URL: &str = "https://api.anthropic.com/v1/messages/count_tokens";
const TOKEN_FIELDS: [&str; 6] = [
    "input",
    "cache_read",
    "cache_write_5m",
    "cache_write_1h",
    "cache_write_unknown",
    "output",
];
const PRICED_FIELDS: [&str; 5] = [
    "input",
    "cache_read",
    "cache_write_5m",
    "cache_write_1h",
    "output",
];

#[derive(Subcommand)]
pub(crate) enum ResearchMeasurementCommand {
    CountTokens(CountTokensArgs),
    ImageTokens(ImageTokensArgs),
    SessionCost(SessionCostArgs),
    ProviderUsage(provider_usage::ProviderUsageArgs),
}

#[derive(Args)]
pub(crate) struct CountTokensArgs {
    #[arg(long)]
    model: String,
    #[arg(long)]
    execute: bool,
    #[arg(long, default_value = "ANTHROPIC_API_KEY")]
    api_key_env: String,
    #[command(subcommand)]
    input: CountInput,
}

#[derive(Subcommand)]
enum CountInput {
    File { path: PathBuf },
    Text { value: String },
    Samples { path: PathBuf },
}

#[derive(Args)]
pub(crate) struct ImageTokensArgs {
    #[arg(long)]
    model: String,
    #[arg(long)]
    execute: bool,
    #[arg(long, default_value = "ANTHROPIC_API_KEY")]
    api_key_env: String,
    #[arg(required = true)]
    dimensions: Vec<String>,
}

#[derive(Args)]
pub(crate) struct SessionCostArgs {
    #[arg(required = true)]
    transcripts: Vec<PathBuf>,
    #[arg(long)]
    prices: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Counts {
    input: u64,
    cache_read: u64,
    cache_write_5m: u64,
    cache_write_1h: u64,
    cache_write_unknown: u64,
    output: u64,
}

impl Counts {
    fn values(&self) -> [u64; 6] {
        [
            self.input,
            self.cache_read,
            self.cache_write_5m,
            self.cache_write_1h,
            self.cache_write_unknown,
            self.output,
        ]
    }

    fn write_total(&self) -> Result<u64> {
        self.cache_write_5m
            .checked_add(self.cache_write_1h)
            .and_then(|total| total.checked_add(self.cache_write_unknown))
            .context("combined cache-write total overflow")
    }

    fn monotone_values(&self) -> Result<[u64; 4]> {
        Ok([
            self.input,
            self.cache_read,
            self.output,
            self.write_total()?,
        ])
    }

    fn to_json(&self) -> Value {
        let values = self.values();
        let mut result = serde_json::Map::new();
        for (field, value) in TOKEN_FIELDS.into_iter().zip(values) {
            result.insert(field.to_owned(), json!(value));
        }
        Value::Object(result)
    }

    fn add_assign(&mut self, other: &Self) -> Result<()> {
        self.input = self
            .input
            .checked_add(other.input)
            .context("input token total overflow")?;
        self.cache_read = self
            .cache_read
            .checked_add(other.cache_read)
            .context("cache-read token total overflow")?;
        self.cache_write_5m = self
            .cache_write_5m
            .checked_add(other.cache_write_5m)
            .context("5m cache-write total overflow")?;
        self.cache_write_1h = self
            .cache_write_1h
            .checked_add(other.cache_write_1h)
            .context("1h cache-write total overflow")?;
        self.cache_write_unknown = self
            .cache_write_unknown
            .checked_add(other.cache_write_unknown)
            .context("unknown-TTL cache-write total overflow")?;
        self.output = self
            .output
            .checked_add(other.output)
            .context("output token total overflow")?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Scope {
    Session(String),
    File(PathBuf),
}

#[derive(Clone, Debug)]
struct Receipt {
    model: String,
    counts: Counts,
}

pub(crate) fn run(command: ResearchMeasurementCommand) -> Result<()> {
    match command {
        ResearchMeasurementCommand::CountTokens(args) => run_count_tokens(args),
        ResearchMeasurementCommand::ImageTokens(args) => run_image_tokens(args),
        ResearchMeasurementCommand::SessionCost(args) => run_session_cost(args),
        ResearchMeasurementCommand::ProviderUsage(args) => provider_usage::run(args),
    }
}

fn run_count_tokens(args: CountTokensArgs) -> Result<()> {
    let samples = load_count_samples(args.input)?;

    let mut output = Vec::with_capacity(samples.len());
    for (label, text) in samples {
        let request_body = count_request_body(&args.model, json!(text));
        let mut row = serde_json::Map::new();
        row.insert("label".to_owned(), json!(label));
        row.insert("model".to_owned(), json!(args.model));
        row.insert("characters".to_owned(), json!(text.chars().count()));
        row.insert("bytes".to_owned(), json!(text.len()));
        let (field, value) = count_request_result(&request_body, args.execute, &args.api_key_env)?;
        row.insert(field.to_owned(), value);
        output.push(Value::Object(row));
    }
    print_json(&output)
}

fn run_image_tokens(args: ImageTokensArgs) -> Result<()> {
    let dimensions = parse_dimension_args(args.dimensions)?;
    let mut rows = Vec::with_capacity(dimensions.len());
    for (dimension, width, height) in dimensions {
        let data = synthetic_png(width, height)?;
        let content = json!([{
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": BASE64.encode(&data),
            }
        }]);
        let request_body = count_request_body(&args.model, content);
        let (field, result) = count_request_result(&request_body, args.execute, &args.api_key_env)?;
        let mut row = serde_json::Map::new();
        row.insert("dimensions".to_owned(), json!(dimension));
        row.insert("model".to_owned(), json!(args.model));
        row.insert("png_bytes".to_owned(), json!(data.len()));
        row.insert(field.to_owned(), result);
        rows.push(Value::Object(row));
    }
    print_json(&rows)
}

fn parse_dimension_args(dimensions: Vec<String>) -> Result<Vec<(String, usize, usize)>> {
    dimensions
        .into_iter()
        .map(|dimension| {
            let (width, height) = parse_dimensions(&dimension)?;
            Ok((dimension, width, height))
        })
        .collect()
}

fn load_count_samples(input: CountInput) -> Result<Vec<(String, String)>> {
    match input {
        CountInput::File { path } => Ok(vec![(path.display().to_string(), read_utf8(&path)?)]),
        CountInput::Text { value } => Ok(vec![("text".to_owned(), value)]),
        CountInput::Samples { path } => {
            let samples = read_utf8(&path)?;
            let samples: Value = serde_json::from_str(&samples)
                .with_context(|| format!("parse samples {}", path.display()))?;
            let rows = samples.as_array().context("samples must be a JSON array")?;
            let mut output = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                let label = row
                    .get("label")
                    .and_then(Value::as_str)
                    .with_context(|| format!("samples[{index}].label must be a string"))?;
                let text = row
                    .get("text")
                    .and_then(Value::as_str)
                    .with_context(|| format!("samples[{index}].text must be a string"))?;
                output.push((label.to_owned(), text.to_owned()));
            }
            Ok(output)
        }
    }
}

fn count_request_result(
    request_body: &Value,
    execute: bool,
    key_env: &str,
) -> Result<(&'static str, Value)> {
    if execute {
        Ok(("response", execute_count_request(request_body, key_env)?))
    } else {
        Ok(("request", request_body.clone()))
    }
}

fn run_session_cost(args: SessionCostArgs) -> Result<()> {
    let (unique_responses, duplicate_records, totals) = collect_receipts(&args.transcripts)?;
    let mut result = serde_json::Map::new();
    result.insert("unique_responses".to_owned(), json!(unique_responses));
    result.insert("duplicate_records".to_owned(), json!(duplicate_records));
    let mut tokens_by_model = serde_json::Map::new();
    for (model, counts) in &totals {
        tokens_by_model.insert(model.clone(), counts.to_json());
    }
    result.insert("tokens_by_model".to_owned(), Value::Object(tokens_by_model));
    result.insert(
        "accounting".to_owned(),
        json!("observed Claude usage; no subscription conversion"),
    );
    if let Some(prices_path) = args.prices {
        let bytes = fs::read(&prices_path)
            .with_context(|| format!("read prices {}", prices_path.display()))?;
        let rates: Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse prices {}", prices_path.display()))?;
        let rates = rates.as_object().context("prices must be a JSON object")?;
        let mut costs = serde_json::Map::new();
        for (model, counts) in &totals {
            costs.insert(model.clone(), price_model(model, counts, rates)?);
        }
        result.insert("list_price_estimate".to_owned(), Value::Object(costs));
    }
    result.insert(
        "limits".to_owned(),
        json!([
            "Explicit files only; include subagents and failed attempts.",
            "Output includes reasoning and tool arguments; no inferred thinking percentage.",
            "This is not a Codex/OpenAI receipt parser; missing input/output receipts fail."
        ]),
    );
    print_json(&Value::Object(result))
}

fn count_request_body(model: &str, content: Value) -> Value {
    json!({
        "model": model,
        "messages": [{"role": "user", "content": content}],
    })
}

fn execute_count_request(body: &Value, key_env: &str) -> Result<Value> {
    let key = env::var(key_env)
        .with_context(|| format!("set the explicitly selected API-key variable {key_env}"))?;
    if key.is_empty() {
        bail!("the explicitly selected API-key variable {key_env} is empty");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build Anthropic token-count client")?;
    let response = client
        .post(ANTHROPIC_COUNT_TOKENS_URL)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(body)
        .send()
        .context("send Anthropic token-count request")?
        .error_for_status()
        .context("Anthropic token-count endpoint returned an error status")?;
    response
        .json()
        .context("parse Anthropic token-count response")
}

fn read_utf8(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", path.display()))
}

fn parse_dimensions(value: &str) -> Result<(usize, usize)> {
    let (width, height) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .with_context(|| format!("invalid dimensions {value:?}; expected WIDTHxHEIGHT"))?;
    if width.contains('x') || width.contains('X') || height.contains('x') || height.contains('X') {
        bail!("invalid dimensions {value:?}; expected WIDTHxHEIGHT");
    }
    let width = width.parse::<usize>().context("invalid PNG width")?;
    let height = height.parse::<usize>().context("invalid PNG height")?;
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        bail!("fixture dimensions must be 1..4096 pixels");
    }
    Ok((width, height))
}

fn png_raw_len(width: usize, height: usize) -> Result<usize> {
    width
        .checked_mul(3)
        .and_then(|row| row.checked_add(1))
        .and_then(|row| row.checked_mul(height))
        .context("PNG dimensions overflow platform limits")
}

fn synthetic_png(width: usize, height: usize) -> Result<Vec<u8>> {
    let raw_len = png_raw_len(width, height)?;
    let row_bytes = width
        .checked_mul(3)
        .context("PNG row length overflow platform limits")?;
    let mut raw = Vec::with_capacity(raw_len);
    for row in 0..height {
        raw.push(0);
        let row_seed = row.checked_mul(7).context("PNG row pattern overflow")?;
        for byte_index in 0..row_bytes {
            let value = row_seed
                .checked_add(byte_index)
                .context("PNG pixel pattern overflow")?;
            raw.push((value & 0xff) as u8);
        }
    }

    let mut compressed = ZlibEncoder::new(Vec::new(), Compression::new(1));
    compressed
        .write_all(&raw)
        .context("compress synthetic PNG data")?;
    let compressed = compressed
        .finish()
        .context("finish synthetic PNG compression")?;

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(
        &u32::try_from(width)
            .context("PNG width exceeds format limit")?
            .to_be_bytes(),
    );
    ihdr.extend_from_slice(
        &u32::try_from(height)
            .context("PNG height exceeds format limit")?
            .to_be_bytes(),
    );
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    append_png_chunk(&mut png, *b"IHDR", &ihdr)?;
    append_png_chunk(&mut png, *b"IDAT", &compressed)?;
    append_png_chunk(&mut png, *b"IEND", &[])?;
    Ok(png)
}

fn append_png_chunk(output: &mut Vec<u8>, kind: [u8; 4], payload: &[u8]) -> Result<()> {
    let length = u32::try_from(payload.len()).context("PNG chunk exceeds format limit")?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&kind);
    output.extend_from_slice(payload);
    let crc_len = kind
        .len()
        .checked_add(payload.len())
        .context("PNG checksum input length overflow")?;
    let mut crc_input = Vec::with_capacity(crc_len);
    crc_input.extend_from_slice(&kind);
    crc_input.extend_from_slice(payload);
    output.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn collect_receipts(paths: &[PathBuf]) -> Result<(usize, usize, BTreeMap<String, Counts>)> {
    let runtime = Builder::new_current_thread()
        .build()
        .context("create transcript reader runtime")?;
    runtime.block_on(collect_receipts_async(paths))
}

async fn collect_receipts_async(
    paths: &[PathBuf],
) -> Result<(usize, usize, BTreeMap<String, Counts>)> {
    let mut seen: BTreeMap<(Scope, String), Receipt> = BTreeMap::new();
    let mut duplicate_records = 0usize;
    for path in paths {
        let file = tokio_fs::File::open(path)
            .await
            .with_context(|| format!("open {}", path.display()))?;
        let mut reader = BufReader::new(file);
        let resolved = tokio_fs::canonicalize(path)
            .await
            .with_context(|| format!("resolve transcript path {}", path.display()))?;
        let mut line = String::new();
        let mut line_number = 0usize;
        loop {
            line.clear();
            let next_line = line_number
                .checked_add(1)
                .context("transcript line counter overflow")?;
            let read = match reader.read_line(&mut line).await {
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::InvalidData => {
                    return Err(error).with_context(|| {
                        format!("{}:{}: invalid UTF-8", path.display(), next_line)
                    });
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("{}:{}: read transcript", path.display(), next_line)
                    });
                }
            };
            if read == 0 {
                break;
            }
            line_number = next_line;
            if line.trim().is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<Value>(&line)
                .with_context(|| format!("{}:{}: invalid JSON", path.display(), line_number))?;
            let parsed_object = parsed.as_object().with_context(|| {
                format!(
                    "{}:{}: record must be an object",
                    path.display(),
                    line_number
                )
            })?;
            if parsed.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let message = match parsed_object
                .get("message")
                .filter(|value| !is_json_falsey(value))
            {
                None => None,
                Some(Value::Object(message)) => Some(message),
                Some(_) => bail!(
                    "{}:{}: message must be an object",
                    path.display(),
                    line_number
                ),
            };
            let Some(message) = message else {
                continue;
            };
            let Some(usage) = message.get("usage").filter(|value| !value.is_null()) else {
                continue;
            };
            let line_result = (|| -> Result<_> {
                let id = message
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .context("usage-bearing response has no message ID")?;
                let session = parsed_object
                    .get("sessionId")
                    .filter(|value| !is_json_falsey(value))
                    .or_else(|| {
                        parsed_object
                            .get("session_id")
                            .filter(|value| !is_json_falsey(value))
                    });
                let scope = match session {
                    None => Scope::File(resolved.clone()),
                    Some(Value::String(session)) => Scope::Session(session.clone()),
                    Some(_) => bail!("session ID must be a string"),
                };
                let model = match message.get("model") {
                    None => "unknown".to_owned(),
                    Some(model) if is_json_falsey(model) => "unknown".to_owned(),
                    Some(Value::String(model)) => model.clone(),
                    Some(_) => bail!("message model must be a string"),
                };
                let counts = normalize_usage(usage)?;
                Ok(((scope, id.to_owned()), Receipt { model, counts }))
            })()
            .with_context(|| format!("{}:{}", path.display(), line_number))?;
            let (key, new) = line_result;
            if let Some(old) = seen.get(&key) {
                duplicate_records = duplicate_records
                    .checked_add(1)
                    .context("duplicate record counter overflow")?;
                if old.model != new.model {
                    bail!(
                        "{}:{}: response model changed for the same ID",
                        path.display(),
                        line_number
                    );
                }
                let old_monotone = old.counts.monotone_values()?;
                let new_monotone = new.counts.monotone_values()?;
                if new_monotone
                    .iter()
                    .zip(old_monotone)
                    .any(|(new, old)| *new < old)
                {
                    bail!(
                        "{}:{}: non-monotone duplicate usage; inspect receipts",
                        path.display(),
                        line_number
                    );
                }
                if new_monotone == old_monotone
                    && new.counts.cache_write_unknown > old.counts.cache_write_unknown
                {
                    continue;
                }
                if new.counts.cache_write_5m < old.counts.cache_write_5m
                    || new.counts.cache_write_1h < old.counts.cache_write_1h
                {
                    bail!(
                        "{}:{}: conflicting cache TTL breakdown for the same ID",
                        path.display(),
                        line_number
                    );
                }
            }
            seen.insert(key, new);
        }
    }

    let mut totals = BTreeMap::<String, Counts>::new();
    for receipt in seen.values() {
        let total = totals.entry(receipt.model.clone()).or_default();
        total.add_assign(&receipt.counts)?;
        total.write_total()?;
    }
    Ok((seen.len(), duplicate_records, totals))
}

fn is_json_falsey(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => true,
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
        Value::Bool(true) => false,
    }
}

fn normalize_usage(usage: &Value) -> Result<Counts> {
    let usage = usage.as_object().context("usage must be an object")?;
    if let Some(iterations) = usage.get("iterations") {
        let iterations = iterations
            .as_array()
            .filter(|iterations| !iterations.is_empty())
            .context("iterations must be a nonempty final usage array")?;
        let mut total = Counts::default();
        for (index, iteration) in iterations.iter().enumerate() {
            let iteration_object = iteration
                .as_object()
                .with_context(|| format!("iterations[{index}] must be an object"))?;
            if iteration_object.contains_key("iterations") {
                bail!("nested usage iterations are invalid");
            }
            total.add_assign(&normalize_usage(iteration)?)?;
        }
        total.write_total()?;
        return Ok(total);
    }
    let required = |field: &str| -> Result<u64> {
        usage
            .get(field)
            .and_then(Value::as_u64)
            .with_context(|| format!("missing or invalid {field}; incomplete usage receipt"))
    };
    let optional = |object: &serde_json::Map<String, Value>, field: &str| -> Result<u64> {
        match object.get(field) {
            None => Ok(0),
            Some(value) => value
                .as_u64()
                .with_context(|| format!("{field} must be a nonnegative integer")),
        }
    };
    let detail = match usage.get("cache_creation") {
        None | Some(Value::Null) => None,
        Some(Value::Object(detail)) => Some(detail),
        Some(_) => bail!("cache_creation must be an object"),
    };
    let cache_write_5m = detail.map_or(Ok(0), |detail| {
        optional(detail, "ephemeral_5m_input_tokens")
    })?;
    let cache_write_1h = detail.map_or(Ok(0), |detail| {
        optional(detail, "ephemeral_1h_input_tokens")
    })?;
    let known = cache_write_5m
        .checked_add(cache_write_1h)
        .context("cache-write breakdown overflow")?;
    let aggregate = match usage.get("cache_creation_input_tokens") {
        None => known,
        Some(value) => value
            .as_u64()
            .context("cache_creation_input_tokens must be a nonnegative integer")?,
    };
    if aggregate < known {
        bail!("cache write breakdown exceeds aggregate");
    }
    let counts = Counts {
        input: required("input_tokens")?,
        cache_read: optional(usage, "cache_read_input_tokens")?,
        cache_write_5m,
        cache_write_1h,
        cache_write_unknown: aggregate - known,
        output: required("output_tokens")?,
    };
    counts.write_total()?;
    Ok(counts)
}

fn price_model(
    model: &str,
    counts: &Counts,
    rates: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let Some(rate) = rates.get(model).filter(|rate| !rate.is_null()) else {
        return Ok(json!({"usd": null, "reason": "no rate for exact model"}));
    };
    let rate = rate
        .as_object()
        .with_context(|| format!("rates for {model} must be an object"))?;
    let mut parsed_rates = Vec::with_capacity(PRICED_FIELDS.len());
    for field in PRICED_FIELDS {
        let number = rate
            .get(field)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .with_context(|| format!("invalid or incomplete rates for {model}"))?;
        parsed_rates.push(number);
    }
    if counts.cache_write_unknown > 0 {
        return Ok(json!({"usd": null, "reason": "cache write TTL unknown"}));
    }
    let values = [
        counts.input,
        counts.cache_read,
        counts.cache_write_5m,
        counts.cache_write_1h,
        counts.output,
    ];
    let mut parts = serde_json::Map::new();
    let mut total = 0.0;
    for ((field, count), rate) in PRICED_FIELDS.into_iter().zip(values).zip(parsed_rates) {
        let part = count as f64 * rate / 1_000_000.0;
        if !part.is_finite() {
            bail!("price estimate overflow for {model}");
        }
        total += part;
        parts.insert(field.to_owned(), json!(part));
    }
    if !total.is_finite() {
        bail!("price estimate overflow for {model}");
    }
    Ok(json!({"usd": total, "by_class": parts}))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value).context("serialize measurement output")?;
    writeln!(output).context("write measurement output newline")?;
    Ok(())
}

mod provider_usage;
