use std::borrow::Cow;
use std::io::Write;
use candle_transformers::generation::Sampling;
use candle_transformers::models::llama::LlamaConfig;
use deno_core::{op2, v8, WebIDL};
use deno_core::{GarbageCollected};
use deno_core::convert::OptionNull;
use deno_core::v8::{Local, PinScope, Value};
use deno_core::webidl::{ContextFn, UnrestrictedDouble, WebIdlConverter, WebIdlError};
/*
struct LanguageModel {}

// SAFETY: we're sure this can be GCed
unsafe impl GarbageCollected for LanguageModel {
  fn trace(&self, _visitor: &mut deno_core::v8::cppgc::Visitor) {}

  fn get_name(&self) -> &'static std::ffi::CStr {
    c"LanguageModel"
  }
}

#[op2]
impl LanguageModel {
  #[async_method]
  #[static_method]
  #[cppgc]
  async fn create(#[webidl] options: LanguageModelCreateOptions) -> LanguageModel {
    LanguageModel {

    }
  }

  #[async_method]
  #[static_method]
  #[cppgc]
  async fn availability(#[webidl] options: LanguageModelCreateCoreOptions) -> LanguageModel {
    LanguageModel {

    }
  }

  #[async_method]
  #[static_method]
  #[cppgc]
  async fn params() -> Option<LanguageModelParams> {
    None
  }

  #[async_method]
  async fn prompt(#[webidl] input: LanguageModelPrompt, #[webidl] options: LanguageModelPromptOptions) -> String {

  }
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelCreateCoreOptions {
  top_k: Option<UnrestrictedDouble>,
  temperature: Option<UnrestrictedDouble>,
  #[webidl(default = vec![])]
  expected_inputs: Vec<LanguageModelExpected>,
  #[webidl(default = vec![])]
  expected_outputs: Vec<LanguageModelExpected>,
  #[webidl(default = vec![])]
  tools: Vec<LanguageModelTool>,
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelCreateOptions {
  top_k: Option<UnrestrictedDouble>,
  temperature: Option<UnrestrictedDouble>,
  #[webidl(default = vec![])]
  expected_inputs: Vec<LanguageModelExpected>,
  #[webidl(default = vec![])]
  expected_outputs: Vec<LanguageModelExpected>,
  #[webidl(default = vec![])]
  tools: Vec<LanguageModelTool>,

  signal: Option<v8::Value>,
  monitor: Option<v8::Function>,
  #[webidl(default = vec![])]
  initial_prompts: Vec<LanguageModelMessage>,
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelMessage {
  role: LanguageModelMessageRole,
  content: StringOrLanguageModelMessageContents,
  #[webidl(default = false)]
  prefix: bool,
}

#[derive(WebIDL)]
#[webidl(enum)]
enum LanguageModelMessageRole {
  System,
  User,
  Assistant,
}

#[derive(WebIDL)]
#[webidl(enum)]
enum LanguageModelMessageType {
  Text,
  Image,
  Audio,
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelMessageContent {
  r#type: LanguageModelMessageType,
  value: LanguageModelMessageValue,
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelTool {
  name: String,
  description: String,
  input_schema: v8::Object,
  execute: v8::Function,
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct LanguageModelExpected {
  r#type: LanguageModelMessageType,
  #[webidl(default = vec![])]
  languages: Vec<String>,
}

enum StringOrLanguageModelMessageContents {
  String(String),
  LanguageModelMessageContents(Vec<LanguageModelMessageContent>),
}

impl<'a> WebIdlConverter<'a> for StringOrLanguageModelMessageContents {
  type Options = ();

  fn convert<'b, 'i>(scope: &mut PinScope<'a, 'i>, value: Local<'a, Value>, prefix: Cow<'static, str>, context: ContextFn<'b>, _: &Self::Options) -> Result<Self, WebIdlError> {
    if value.is_array() {
      Ok(StringOrLanguageModelMessageContents::LanguageModelMessageContents(WebIdlConverter::convert(scope, value, prefix, context, &Default::default())?))
    } else {
      Ok(StringOrLanguageModelMessageContents::String(WebIdlConverter::convert(scope, value, prefix, context, &Default::default())?))
    }
  }
}

enum LanguageModelMessageValue {
  String(String),
  BufferSource(BufferSource), // TODO
}

impl<'a> WebIdlConverter<'a> for LanguageModelMessageValue {
  type Options = ();

  fn convert<'b, 'i>(scope: &mut PinScope<'a, 'i>, value: Local<'a, Value>, prefix: Cow<'static, str>, context: ContextFn<'b>, _: &Self::Options) -> Result<Self, WebIdlError> {
    if value.is_array() {
      Ok(LanguageModelMessageValue::BufferSource(WebIdlConverter::convert(scope, value, prefix, context, &Default::default())?))
    } else {
      Ok(LanguageModelMessageValue::String(WebIdlConverter::convert(scope, value, prefix, context, &Default::default())?))
    }
  }
}*/

struct LanguageModelParams {
  default_top_k: u32,
  max_top_k: u32,
  default_temperature: f32,
  max_temperature: f32,
}

// SAFETY: we're sure this can be GCed
unsafe impl GarbageCollected for LanguageModelParams {
  fn trace(&self, _visitor: &mut deno_core::v8::cppgc::Visitor) {}

  fn get_name(&self) -> &'static std::ffi::CStr {
    c"LanguageModelParams"
  }
}


#[op2]
impl LanguageModelParams {
  #[getter]
  fn default_top_k(&self) -> u32 {
    self.default_top_k
  }

  #[getter]
  fn max_top_k(&self) -> u32 {
    self.max_top_k
  }

  #[getter]
  fn default_temperature(&self) -> f32 {
    self.default_temperature
  }

  #[getter]
  fn max_temperature(&self) -> f32 {
    self.max_temperature
  }
}

#[op2(async)]
#[string]
pub async fn ml_prompt(#[string] prompt: String) -> String {
  let device = if candle_core::utils::cuda_is_available() {
    candle_core::Device::new_cuda(0).unwrap()
  } else if candle_core::utils::metal_is_available() {
    candle_core::Device::new_metal(0).unwrap()
  } else {
    candle_core::Device::Cpu
  };

  let dtype = candle_core::DType::F16;
  let (llama, tokenizer_filename, mut cache, config) = {
    let api = hf_hub::api::tokio::ApiBuilder::new()
      .with_progress(false)
      .build().unwrap();
    let model_id = "HuggingFaceTB/SmolLM2-360M-Instruct";
    let api = api.repo(hf_hub::Repo::with_revision(model_id.to_string(), hf_hub::RepoType::Model, "main".to_string()));

    let tokenizer_filename = api.get("tokenizer.json").await.unwrap();
    let config_filename = api.get("config.json").await.unwrap();
    let config: LlamaConfig = deno_core::serde_json::from_slice(&std::fs::read(config_filename).unwrap()).unwrap();
    let config = config.into_config(false);

    let json_file = api.get("model.safetensors").await.unwrap();
    /*let json_file = std::fs::File::open(json_file).unwrap();
    let json: deno_core::serde_json::Value =
      deno_core::serde_json::from_reader(&json_file).unwrap();
    let weight_map = match json.get("weight_map") {
      Some(deno_core::serde_json::Value::Object(map)) => map,
      _ => unreachable!(),
    };
    let mut safetensors_files = std::collections::HashSet::new();
    for value in weight_map.values() {
      if let Some(file) = value.as_str() {
        safetensors_files.insert(file.to_string());
      }
    }

    for safetensors_file in &safetensors_files {
      api.get(safetensors_file).await.unwrap();
    }
    let safetensors_files = safetensors_files.into_iter().collect::<Vec<_>>();
*/
    let safetensors_files = vec![json_file];


    let cache = candle_transformers::models::llama::Cache::new(true, dtype, &config, &device).unwrap();

    let vb = unsafe { candle_nn::VarBuilder::from_mmaped_safetensors(&safetensors_files, dtype, &device).unwrap() };
    (candle_transformers::models::llama::Llama::load(vb, &config).unwrap(), tokenizer_filename, cache, config)
  };
  let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_filename).unwrap();
  let eos_token_id = config.eos_token_id.or_else(|| {
    let mut tokens = vec![];

    if let Some(token) = tokenizer.token_to_id("<|end_of_text|>") {
      tokens.push(token);
    }
    if let Some(token) = tokenizer.token_to_id("<|eot_id|>") {
      tokens.push(token);
    }
    if let Some(token) = tokenizer.token_to_id("</s>") {
      tokens.push(token);
    }

    if tokens.is_empty() {
      None
    } else {
      Some(candle_transformers::models::llama::LlamaEosToks::Multiple(tokens))
    }
  });

  let mut tokens = tokenizer
    .encode(format!("You are a helpful assistant.\nUser: {prompt}\nAssistant:"), false)
    .unwrap()
    .get_ids()
    .to_vec();
  let mut tokenizer = TokenOutputStream::new(tokenizer);

  let mut logits_processor = {
    let temperature = 0.8;
    let sampling = if temperature <= 0. {
      Sampling::ArgMax
    } else {
      let top_k = None;
      let top_p = None;
      match (top_k, top_p) {
        (None, None) => Sampling::All { temperature },
        (Some(k), None) => Sampling::TopK { k, temperature },
        (None, Some(p)) => Sampling::TopP { p, temperature },
        (Some(k), Some(p)) => Sampling::TopKThenTopP { k, p, temperature },
      }
    };
    candle_transformers::generation::LogitsProcessor::from_sampling(299792458, sampling)
  };

  let mut start_gen = std::time::Instant::now();
  let mut index_pos = 0;
  let mut token_generated = 0;

  let mut out = String::new();

  for index in 0..10000 {
    let (context_size, context_index) = if cache.use_kv_cache && index > 0 {
      (1, index_pos)
    } else {
      (tokens.len(), 0)
    };
    if index == 1 {
      start_gen = std::time::Instant::now()
    }
    let ctxt = &tokens[tokens.len().saturating_sub(context_size)..];
    let input = candle_core::Tensor::new(ctxt, &device).unwrap().unsqueeze(0).unwrap();
    let logits = llama.forward(&input, context_index, &mut cache).unwrap();
    let logits = logits.squeeze(0).unwrap();
    let repeat_penalty = 1.1;
    let logits = if repeat_penalty == 1. {
      logits
    } else {
      let start_at = tokens.len().saturating_sub(128);
      candle_transformers::utils::apply_repeat_penalty(
        &logits,
        repeat_penalty,
        &tokens[start_at..],
      ).unwrap()
    };
    index_pos += ctxt.len();

    let next_token = logits_processor.sample(&logits).unwrap();
    token_generated += 1;
    tokens.push(next_token);

    match eos_token_id {
      Some(candle_transformers::models::llama::LlamaEosToks::Single(eos_tok_id)) if next_token == eos_tok_id => {
        break;
      }
      Some(candle_transformers::models::llama::LlamaEosToks::Multiple(ref eos_ids)) if eos_ids.contains(&next_token) => {
        break;
      }
      _ => (),
    }
    if let Some(t) = tokenizer.next_token(next_token).unwrap() {
      out.push_str(&t);
    }
  }

  if let Some(rest) = tokenizer.decode_rest().unwrap() {
    out.push_str(&rest);
  }

  out
}

pub struct TokenOutputStream {
  tokenizer: tokenizers::Tokenizer,
  tokens: Vec<u32>,
  prev_index: usize,
  current_index: usize,
}

impl TokenOutputStream {
  pub fn new(tokenizer: tokenizers::Tokenizer) -> Self {
    Self {
      tokenizer,
      tokens: Vec::new(),
      prev_index: 0,
      current_index: 0,
    }
  }

  pub fn into_inner(self) -> tokenizers::Tokenizer {
    self.tokenizer
  }

  fn decode(&self, tokens: &[u32]) -> candle_core::Result<String> {
    match self.tokenizer.decode(tokens, true) {
      Ok(str) => Ok(str),
      Err(err) => candle_core::bail!("cannot decode: {err}"),
    }
  }

  // https://github.com/huggingface/text-generation-inference/blob/5ba53d44a18983a4de32d122f4cb46f4a17d9ef6/server/text_generation_server/models/model.py#L68
  pub fn next_token(&mut self, token: u32) -> candle_core::Result<Option<String>> {
    let prev_text = if self.tokens.is_empty() {
      String::new()
    } else {
      let tokens = &self.tokens[self.prev_index..self.current_index];
      self.decode(tokens)?
    };
    self.tokens.push(token);
    let text = self.decode(&self.tokens[self.prev_index..])?;
    if text.len() > prev_text.len() && text.chars().last().unwrap().is_alphanumeric() {
      let text = text.split_at(prev_text.len());
      self.prev_index = self.current_index;
      self.current_index = self.tokens.len();
      Ok(Some(text.1.to_string()))
    } else {
      Ok(None)
    }
  }

  pub fn decode_rest(&self) -> candle_core::Result<Option<String>> {
    let prev_text = if self.tokens.is_empty() {
      String::new()
    } else {
      let tokens = &self.tokens[self.prev_index..self.current_index];
      self.decode(tokens)?
    };
    let text = self.decode(&self.tokens[self.prev_index..])?;
    if text.len() > prev_text.len() {
      let text = text.split_at(prev_text.len());
      Ok(Some(text.1.to_string()))
    } else {
      Ok(None)
    }
  }

  pub fn decode_all(&self) -> candle_core::Result<String> {
    self.decode(&self.tokens)
  }

  pub fn get_token(&self, token_s: &str) -> Option<u32> {
    self.tokenizer.get_vocab(true).get(token_s).copied()
  }

  pub fn tokenizer(&self) -> &tokenizers::Tokenizer {
    &self.tokenizer
  }

  pub fn clear(&mut self) {
    self.tokens.clear();
    self.prev_index = 0;
    self.current_index = 0;
  }
}
