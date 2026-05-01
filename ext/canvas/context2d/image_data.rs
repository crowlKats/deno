// Copyright 2018-2026 the Deno authors. MIT license.

use std::cell::RefCell;

use deno_core::GarbageCollected;
use deno_core::WebIDL;
use deno_core::op2;
use deno_core::v8;
use deno_core::v8::cppgc::Visitor;
use deno_core::webidl::WebIdlInterfaceConverter;
use deno_error::JsErrorBox;

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum PredefinedColorSpace {
  Srgb,
  DisplayP3,
}

pub struct ImageData {
  pub width: u32,
  pub height: u32,
  pub color_space: PredefinedColorSpace,
  /// RGBA8, length = width * height * 4. Wrapped in a RefCell so JS code can
  /// keep a (logical) Uint8ClampedArray view that the harness re-creates from
  /// a backing store on access.
  pub data: RefCell<Vec<u8>>,
}

// SAFETY: no v8 references.
unsafe impl GarbageCollected for ImageData {
  fn trace(&self, _visitor: &mut Visitor) {}
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"ImageData"
  }
}

impl WebIdlInterfaceConverter for ImageData {
  const NAME: &'static str = "ImageData";
}

#[op2]
impl ImageData {
  /// `new ImageData(width, height, settings?)` or
  /// `new ImageData(data, width, height?, settings?)`.
  #[constructor]
  #[cppgc]
  fn new<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
    arg3: v8::Local<'s, v8::Value>,
  ) -> Result<ImageData, JsErrorBox> {
    // Detect form 2 first: arg0 is a Uint8ClampedArray.
    if arg0.is_uint8_clamped_array() {
      let arr: v8::Local<v8::Uint8Array> = arg0
        .try_into()
        .map_err(|_| JsErrorBox::type_error("expected Uint8ClampedArray"))?;
      let len = arr.byte_length();
      if len % 4 != 0 {
        return Err(JsErrorBox::new(
          "DOMExceptionInvalidStateError",
          "ImageData buffer length must be a multiple of 4",
        ));
      }
      let pixels = len / 4;
      let width = arg1.uint32_value(scope).map(|v| v as u32).unwrap_or(0);
      if width == 0 {
        return Err(JsErrorBox::new(
          "DOMExceptionIndexSizeError",
          "ImageData width must be > 0",
        ));
      }
      if (pixels as u32) % width != 0 {
        return Err(JsErrorBox::new(
          "DOMExceptionIndexSizeError",
          "ImageData buffer is not a multiple of width",
        ));
      }
      let derived_h = (pixels as u32) / width;
      let height = if arg2.is_undefined() {
        derived_h
      } else {
        let h = arg2.uint32_value(scope).map(|v| v as u32).unwrap_or(0);
        if h == 0 {
          return Err(JsErrorBox::new(
            "DOMExceptionIndexSizeError",
            "ImageData height must be > 0",
          ));
        }
        if h != derived_h {
          return Err(JsErrorBox::new(
            "DOMExceptionIndexSizeError",
            "ImageData height does not match buffer length",
          ));
        }
        h
      };
      let color_space = parse_color_space(scope, arg3);
      let mut data = vec![0u8; len];
      let buf = arr.buffer(scope).ok_or_else(|| {
        JsErrorBox::type_error("Uint8ClampedArray has no backing store")
      })?;
      let store = buf.get_backing_store();
      let offset = arr.byte_offset();
      // SAFETY: we copy out of the JS-owned buffer immediately.
      let src = unsafe {
        std::slice::from_raw_parts(
          (store.data().unwrap().as_ptr() as *const u8).add(offset),
          len,
        )
      };
      data.copy_from_slice(src);
      return Ok(ImageData {
        width,
        height,
        color_space,
        data: RefCell::new(data),
      });
    }

    // Form 1: width, height [, settings]
    let width = arg0.uint32_value(scope).map(|v| v as u32).unwrap_or(0);
    let height = arg1.uint32_value(scope).map(|v| v as u32).unwrap_or(0);
    if width == 0 || height == 0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "ImageData dimensions must be > 0",
      ));
    }
    let color_space = parse_color_space(scope, arg2);
    let len = (width as usize)
      .checked_mul(height as usize)
      .and_then(|x| x.checked_mul(4))
      .ok_or_else(|| {
        JsErrorBox::new(
          "DOMExceptionRangeError",
          "ImageData dimensions overflow",
        )
      })?;
    Ok(ImageData {
      width,
      height,
      color_space,
      data: RefCell::new(vec![0; len]),
    })
  }

  #[getter]
  fn width(&self) -> u32 {
    self.width
  }

  #[getter]
  fn height(&self) -> u32 {
    self.height
  }

  #[getter]
  #[string]
  fn color_space(&self) -> &'static str {
    match self.color_space {
      PredefinedColorSpace::Srgb => "srgb",
      PredefinedColorSpace::DisplayP3 => "display-p3",
    }
  }

  #[getter]
  fn data<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> v8::Local<'s, v8::Uint8ClampedArray> {
    let data = self.data.borrow();
    let len = data.len();
    let bs = v8::ArrayBuffer::new_backing_store_from_vec(data.clone());
    let shared = bs.make_shared();
    let ab = v8::ArrayBuffer::with_backing_store(scope, &shared);
    v8::Uint8ClampedArray::new(scope, ab, 0, len).unwrap()
  }
}

fn parse_color_space<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> PredefinedColorSpace {
  if value.is_undefined() || value.is_null() {
    return PredefinedColorSpace::Srgb;
  }
  let Ok(obj) = TryInto::<v8::Local<v8::Object>>::try_into(value) else {
    return PredefinedColorSpace::Srgb;
  };
  let k = v8::String::new(scope, "colorSpace").unwrap();
  let Some(v) = obj.get(scope, k.into()) else {
    return PredefinedColorSpace::Srgb;
  };
  if v.is_undefined() {
    return PredefinedColorSpace::Srgb;
  }
  let s = v.to_rust_string_lossy(scope);
  match s.as_str() {
    "display-p3" => PredefinedColorSpace::DisplayP3,
    _ => PredefinedColorSpace::Srgb,
  }
}
