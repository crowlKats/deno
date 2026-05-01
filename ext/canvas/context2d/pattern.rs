// Copyright 2018-2026 the Deno authors. MIT license.

use std::cell::RefCell;

use deno_core::GarbageCollected;
use deno_core::WebIDL;
use deno_core::op2;
use deno_core::v8;
use deno_core::v8::cppgc::Visitor;
use deno_core::webidl::WebIdlInterfaceConverter;
use deno_error::JsErrorBox;
use vello::kurbo::Affine;
use vello::peniko::Blob;
use vello::peniko::Brush;
use vello::peniko::Extend;
use vello::peniko::Image;
use vello::peniko::ImageFormat;

use super::path::parse_dom_matrix_init;

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum PatternRepetition {
  Repeat,
  RepeatX,
  RepeatY,
  NoRepeat,
}

impl PatternRepetition {
  pub fn from_string(s: &str) -> Option<Self> {
    match s {
      "" | "repeat" => Some(PatternRepetition::Repeat),
      "repeat-x" => Some(PatternRepetition::RepeatX),
      "repeat-y" => Some(PatternRepetition::RepeatY),
      "no-repeat" => Some(PatternRepetition::NoRepeat),
      _ => None,
    }
  }

  fn extends(self) -> (Extend, Extend) {
    match self {
      PatternRepetition::Repeat => (Extend::Repeat, Extend::Repeat),
      PatternRepetition::RepeatX => (Extend::Repeat, Extend::Pad),
      PatternRepetition::RepeatY => (Extend::Pad, Extend::Repeat),
      PatternRepetition::NoRepeat => (Extend::Pad, Extend::Pad),
    }
  }
}

pub struct CanvasPattern {
  pub width: u32,
  pub height: u32,
  pub rgba: Vec<u8>,
  pub repetition: PatternRepetition,
  pub transform: RefCell<Affine>,
}

// SAFETY: no v8 references.
unsafe impl GarbageCollected for CanvasPattern {
  fn trace(&self, _visitor: &mut Visitor) {}
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"CanvasPattern"
  }
}

impl WebIdlInterfaceConverter for CanvasPattern {
  const NAME: &'static str = "CanvasPattern";
}

#[op2]
impl CanvasPattern {
  #[nofast]
  #[reentrant]
  fn set_transform<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    transform: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let t = parse_dom_matrix_init(scope, transform)?;
    *self.transform.borrow_mut() = t;
    Ok(())
  }
}

impl CanvasPattern {
  /// Build a peniko brush. The image data is cloned into a peniko `Blob`,
  /// then wrapped as an `Image`.
  pub fn to_brush(&self) -> Brush {
    let blob = Blob::new(std::sync::Arc::new(self.rgba.clone()));
    let img = Image::new(blob, ImageFormat::Rgba8, self.width, self.height);
    let (xe, ye) = self.repetition.extends();
    Brush::Image(img.with_x_extend(xe).with_y_extend(ye))
  }
}
