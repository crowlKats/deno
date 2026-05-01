// Copyright 2018-2026 the Deno authors. MIT license.

import {
  CanvasGradient,
  CanvasPattern,
  CanvasRenderingContext2D,
  ImageBitmapRenderingContext,
  ImageData,
  OffscreenCanvas,
  op_init_canvas,
  Path2D,
  TextMetrics,
} from "ext:core/ops";
import { Blob } from "ext:deno_web/09_file.js";

op_init_canvas(Blob);

export {
  CanvasGradient,
  CanvasPattern,
  CanvasRenderingContext2D,
  ImageBitmapRenderingContext,
  ImageData,
  OffscreenCanvas,
  Path2D,
  TextMetrics,
};
