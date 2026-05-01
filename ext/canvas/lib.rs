// Copyright 2018-2026 the Deno authors. MIT license.

use deno_core::OpState;
use deno_core::op2;
use deno_core::v8;

mod bitmaprenderer;
mod byow;
mod canvas;
mod context2d;

deno_core::extension!(
  deno_canvas,
  deps = [deno_webidl, deno_web, deno_webgpu],
  ops = [op_init_canvas],
  objects = [
    bitmaprenderer::ImageBitmapRenderingContext,
    context2d::CanvasRenderingContext2D,
    context2d::CanvasGradient,
    context2d::CanvasPattern,
    context2d::ImageData,
    context2d::Path2D,
    context2d::TextMetrics,
    canvas::OffscreenCanvas,
    byow::UnsafeWindowSurface,
  ],
  esm = ["02_surface.js"],
  lazy_loaded_esm = ["01_canvas.js"],
);

#[op2(fast)]
pub fn op_init_canvas(
  state: &mut OpState,
  scope: &mut v8::PinScope<'_, '_>,
  blob: v8::Local<v8::Value>,
) {
  state.put(canvas::BlobHandle(v8::Global::new(scope, blob.cast())));
}
