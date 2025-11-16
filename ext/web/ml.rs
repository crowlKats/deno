use std::borrow::Cow;
use deno_core::{op2, v8, WebIDL};
use deno_core::{GarbageCollected};
use deno_core::convert::OptionNull;
use deno_core::v8::{Local, PinScope, Value};
use deno_core::webidl::{ContextFn, UnrestrictedDouble, WebIdlConverter, WebIdlError};

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
}

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
