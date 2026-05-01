// Copyright 2018-2026 the Deno authors. MIT license.
//
// CPU post-processing pipeline for the 2d context. Vello produces an RGBA8
// overlay; we run filters and the shadow pass on it before compositing onto
// the canvas's persistent bitmap.

use deno_image::image::RgbaImage;

use super::state::Color;
use super::state::parse_color;

#[derive(Clone, Debug)]
pub enum FilterFn {
  Blur(f64),
  Brightness(f64),
  Contrast(f64),
  DropShadow {
    dx: f64,
    dy: f64,
    blur: f64,
    color: Color,
  },
  Grayscale(f64),
  HueRotate(f64),
  Invert(f64),
  Opacity(f64),
  Saturate(f64),
  Sepia(f64),
}

/// Parse a CSS `filter` value. Returns an empty vec for "none" / "" / parse
/// errors; this matches browser behaviour of ignoring an unparsable filter.
pub fn parse_filter(s: &str) -> Vec<FilterFn> {
  let s = s.trim();
  if s.is_empty() || s.eq_ignore_ascii_case("none") {
    return Vec::new();
  }
  let mut out = Vec::new();
  let bytes = s.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= bytes.len() {
      break;
    }
    let name_start = i;
    while i < bytes.len() && bytes[i] != b'(' {
      i += 1;
    }
    if i >= bytes.len() {
      return Vec::new();
    }
    let name = s[name_start..i].trim().to_ascii_lowercase();
    i += 1; // skip '('
    let arg_start = i;
    let mut depth = 1;
    while i < bytes.len() && depth > 0 {
      match bytes[i] {
        b'(' => depth += 1,
        b')' => depth -= 1,
        _ => {}
      }
      i += 1;
    }
    if depth != 0 {
      return Vec::new();
    }
    let args = &s[arg_start..i - 1];
    let Some(f) = parse_one(&name, args) else {
      return Vec::new();
    };
    out.push(f);
  }
  out
}

fn parse_one(name: &str, args: &str) -> Option<FilterFn> {
  match name {
    "blur" => Some(FilterFn::Blur(parse_length_px(args)?)),
    "brightness" => Some(FilterFn::Brightness(parse_factor(args)?)),
    "contrast" => Some(FilterFn::Contrast(parse_factor(args)?)),
    "grayscale" => {
      Some(FilterFn::Grayscale(parse_factor(args)?.clamp(0.0, 1.0)))
    }
    "invert" => Some(FilterFn::Invert(parse_factor(args)?.clamp(0.0, 1.0))),
    "opacity" => Some(FilterFn::Opacity(parse_factor(args)?.clamp(0.0, 1.0))),
    "saturate" => Some(FilterFn::Saturate(parse_factor(args)?)),
    "sepia" => Some(FilterFn::Sepia(parse_factor(args)?.clamp(0.0, 1.0))),
    "hue-rotate" => Some(FilterFn::HueRotate(parse_angle_rad(args)?)),
    "drop-shadow" => parse_drop_shadow(args),
    _ => None,
  }
}

fn parse_length_px(s: &str) -> Option<f64> {
  let s = s.trim();
  if s.is_empty() {
    return Some(0.0);
  }
  if let Some(num) = s.strip_suffix("px") {
    num.trim().parse::<f64>().ok()
  } else {
    s.parse::<f64>().ok()
  }
}

fn parse_factor(s: &str) -> Option<f64> {
  let s = s.trim();
  if s.is_empty() {
    return Some(1.0);
  }
  if let Some(p) = s.strip_suffix('%') {
    return Some(p.trim().parse::<f64>().ok()? / 100.0);
  }
  s.parse::<f64>().ok()
}

fn parse_angle_rad(s: &str) -> Option<f64> {
  let s = s.trim();
  if s.is_empty() {
    return Some(0.0);
  }
  for (suffix, factor) in [
    ("deg", std::f64::consts::PI / 180.0),
    ("grad", std::f64::consts::PI / 200.0),
    ("rad", 1.0),
    ("turn", std::f64::consts::TAU),
  ] {
    if let Some(num) = s.strip_suffix(suffix) {
      return Some(num.trim().parse::<f64>().ok()? * factor);
    }
  }
  s.parse::<f64>().ok()
}

fn parse_drop_shadow(args: &str) -> Option<FilterFn> {
  // `<length> <length> [<length>] <color>` — color may be at start or end.
  // Naive tokenizer: split on whitespace not inside parens.
  let tokens = split_top_level(args);
  if tokens.len() < 3 {
    return None;
  }
  let mut numeric = Vec::with_capacity(3);
  let mut color_token = None;
  for tok in &tokens {
    if numeric.len() < 3 {
      if let Some(v) = parse_length_px(tok) {
        numeric.push(v);
        continue;
      }
    }
    color_token = Some(tok.as_str());
  }
  if numeric.len() < 2 {
    return None;
  }
  let color = match color_token {
    Some(c) => parse_color(c)?,
    None => Color::BLACK,
  };
  Some(FilterFn::DropShadow {
    dx: numeric[0],
    dy: numeric[1],
    blur: numeric.get(2).copied().unwrap_or(0.0),
    color,
  })
}

fn split_top_level(s: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut depth = 0i32;
  let mut buf = String::new();
  for c in s.chars() {
    match c {
      '(' => {
        depth += 1;
        buf.push(c);
      }
      ')' => {
        depth -= 1;
        buf.push(c);
      }
      c if c.is_whitespace() && depth == 0 => {
        if !buf.is_empty() {
          out.push(std::mem::take(&mut buf));
        }
      }
      c => buf.push(c),
    }
  }
  if !buf.is_empty() {
    out.push(buf);
  }
  out
}

/// Apply a filter chain in-place to an RGBA8 buffer.
pub fn apply_filters(
  buf: &mut Vec<u8>,
  width: u32,
  height: u32,
  filters: &[FilterFn],
) {
  for f in filters {
    match f {
      FilterFn::Blur(r) => gaussian_blur(buf, width, height, *r),
      FilterFn::Brightness(v) => map_rgb(buf, |c| (c * *v).clamp(0.0, 1.0)),
      FilterFn::Contrast(v) => {
        map_rgb(buf, |c| ((c - 0.5) * *v + 0.5).clamp(0.0, 1.0))
      }
      FilterFn::Grayscale(v) => {
        apply_color_matrix(buf, &lerp_matrix(IDENTITY, GRAYSCALE, *v))
      }
      FilterFn::Invert(v) => map_rgb(buf, |c| (1.0 - 2.0 * *v) * c + *v),
      FilterFn::Opacity(v) => {
        for px in buf.chunks_exact_mut(4) {
          px[3] = ((px[3] as f64) * *v).clamp(0.0, 255.0) as u8;
        }
      }
      FilterFn::Saturate(v) => apply_color_matrix(buf, &saturation_matrix(*v)),
      FilterFn::Sepia(v) => {
        apply_color_matrix(buf, &lerp_matrix(IDENTITY, SEPIA, *v))
      }
      FilterFn::HueRotate(rad) => {
        apply_color_matrix(buf, &hue_rotate_matrix(*rad))
      }
      FilterFn::DropShadow {
        dx,
        dy,
        blur,
        color,
      } => {
        let shadow = make_shadow(buf, width, height, *color, *blur);
        // Composite the shadow under the existing buffer.
        let mut combined = vec![0u8; buf.len()];
        composite_offset(&mut combined, &shadow, width, height, *dx, *dy);
        composite_over_inplace(&mut combined, buf);
        std::mem::swap(buf, &mut combined);
      }
    }
  }
}

/// Build a shadow buffer from `overlay`'s alpha channel, tinted with `color`
/// and blurred by `blur` pixels (sigma = blur/2 per spec).
pub fn make_shadow(
  overlay: &[u8],
  width: u32,
  height: u32,
  color: Color,
  blur: f64,
) -> Vec<u8> {
  let mut buf = vec![0u8; overlay.len()];
  let [r, g, b, a] = color.to_rgba8().to_u8_array();
  for (dst, src) in buf.chunks_exact_mut(4).zip(overlay.chunks_exact(4)) {
    let sa = src[3] as u32;
    let oa = (sa * a as u32) / 255;
    if oa == 0 {
      continue;
    }
    // Premultiplied colors based on the shadow color.
    dst[0] = ((r as u32 * oa) / 255) as u8;
    dst[1] = ((g as u32 * oa) / 255) as u8;
    dst[2] = ((b as u32 * oa) / 255) as u8;
    dst[3] = oa as u8;
  }
  // Caller's buf is in straight-alpha format; blur in straight alpha.
  // Convert premultiplied -> straight before blur.
  for px in buf.chunks_exact_mut(4) {
    if px[3] == 0 {
      continue;
    }
    let a = px[3] as u32;
    px[0] = ((px[0] as u32 * 255) / a).min(255) as u8;
    px[1] = ((px[1] as u32 * 255) / a).min(255) as u8;
    px[2] = ((px[2] as u32 * 255) / a).min(255) as u8;
  }
  if blur > 0.0 {
    gaussian_blur(&mut buf, width, height, blur);
  }
  buf
}

/// Composite `overlay` (straight RGBA) onto `base` at `(offset_x, offset_y)`.
/// `base` must have the same dimensions as `overlay`.
pub fn composite_overlay_at(
  base: &mut RgbaImage,
  overlay: &[u8],
  offset_x: f64,
  offset_y: f64,
) {
  let bw = base.width() as i64;
  let bh = base.height() as i64;
  let ox = offset_x.round() as i64;
  let oy = offset_y.round() as i64;
  let buf = base.as_flat_samples_mut().samples;
  for j in 0..bh {
    let sy = j - oy;
    if sy < 0 || sy >= bh {
      continue;
    }
    for i in 0..bw {
      let sx = i - ox;
      if sx < 0 || sx >= bw {
        continue;
      }
      let s_off = ((sy * bw + sx) * 4) as usize;
      let d_off = ((j * bw + i) * 4) as usize;
      let sa = overlay[s_off + 3] as u32;
      if sa == 0 {
        continue;
      }
      let inv = 255 - sa;
      for c in 0..3 {
        let s = overlay[s_off + c] as u32;
        let d = buf[d_off + c] as u32;
        buf[d_off + c] = ((s * sa + d * inv) / 255) as u8;
      }
      let da = buf[d_off + 3] as u32;
      buf[d_off + 3] = (sa + (da * inv) / 255) as u8;
    }
  }
}

/// Composite `src` over `dst` in-place, both RGBA8 of the same dimensions.
fn composite_over_inplace(dst: &mut [u8], src: &[u8]) {
  for i in (0..dst.len()).step_by(4) {
    let sa = src[i + 3] as u32;
    if sa == 0 {
      continue;
    }
    let inv = 255 - sa;
    for c in 0..3 {
      let s = src[i + c] as u32;
      let d = dst[i + c] as u32;
      dst[i + c] = ((s * sa + d * inv) / 255) as u8;
    }
    let da = dst[i + 3] as u32;
    dst[i + 3] = (sa + (da * inv) / 255) as u8;
  }
}

/// Composite `src` onto `dst` (same size) shifted by `(dx, dy)`. Used for
/// drop-shadow's offset. `dst` is assumed pre-zeroed.
fn composite_offset(
  dst: &mut [u8],
  src: &[u8],
  width: u32,
  height: u32,
  dx: f64,
  dy: f64,
) {
  let w = width as i64;
  let h = height as i64;
  let ox = dx.round() as i64;
  let oy = dy.round() as i64;
  for j in 0..h {
    let sy = j - oy;
    if sy < 0 || sy >= h {
      continue;
    }
    for i in 0..w {
      let sx = i - ox;
      if sx < 0 || sx >= w {
        continue;
      }
      let s_off = ((sy * w + sx) * 4) as usize;
      let d_off = ((j * w + i) * 4) as usize;
      dst[d_off] = src[s_off];
      dst[d_off + 1] = src[s_off + 1];
      dst[d_off + 2] = src[s_off + 2];
      dst[d_off + 3] = src[s_off + 3];
    }
  }
}

// ---- color transforms ----------------------------------------------------

fn map_rgb<F: Fn(f64) -> f64>(buf: &mut [u8], f: F) {
  for px in buf.chunks_exact_mut(4) {
    if px[3] == 0 {
      continue;
    }
    for c in 0..3 {
      let v = px[c] as f64 / 255.0;
      px[c] = (f(v).clamp(0.0, 1.0) * 255.0) as u8;
    }
  }
}

type Mat = [[f64; 4]; 3];

const IDENTITY: Mat = [
  [1.0, 0.0, 0.0, 0.0],
  [0.0, 1.0, 0.0, 0.0],
  [0.0, 0.0, 1.0, 0.0],
];

const GRAYSCALE: Mat = [
  [0.2126, 0.7152, 0.0722, 0.0],
  [0.2126, 0.7152, 0.0722, 0.0],
  [0.2126, 0.7152, 0.0722, 0.0],
];

const SEPIA: Mat = [
  [0.393, 0.769, 0.189, 0.0],
  [0.349, 0.686, 0.168, 0.0],
  [0.272, 0.534, 0.131, 0.0],
];

fn lerp_matrix(a: Mat, b: Mat, t: f64) -> Mat {
  let mut out = [[0.0; 4]; 3];
  for i in 0..3 {
    for j in 0..4 {
      out[i][j] = a[i][j] * (1.0 - t) + b[i][j] * t;
    }
  }
  out
}

fn saturation_matrix(s: f64) -> Mat {
  // Standard saturate matrix from CSS Filter Effects.
  let r = 0.213;
  let g = 0.715;
  let b = 0.072;
  [
    [r + (1.0 - r) * s, g - g * s, b - b * s, 0.0],
    [r - r * s, g + (1.0 - g) * s, b - b * s, 0.0],
    [r - r * s, g - g * s, b + (1.0 - b) * s, 0.0],
  ]
}

fn hue_rotate_matrix(angle: f64) -> Mat {
  let cos = angle.cos();
  let sin = angle.sin();
  // From CSS Filter Effects spec.
  [
    [
      0.213 + 0.787 * cos - 0.213 * sin,
      0.715 - 0.715 * cos - 0.715 * sin,
      0.072 - 0.072 * cos + 0.928 * sin,
      0.0,
    ],
    [
      0.213 - 0.213 * cos + 0.143 * sin,
      0.715 + 0.285 * cos + 0.140 * sin,
      0.072 - 0.072 * cos - 0.283 * sin,
      0.0,
    ],
    [
      0.213 - 0.213 * cos - 0.787 * sin,
      0.715 - 0.715 * cos + 0.715 * sin,
      0.072 + 0.928 * cos + 0.072 * sin,
      0.0,
    ],
  ]
}

fn apply_color_matrix(buf: &mut [u8], m: &Mat) {
  for px in buf.chunks_exact_mut(4) {
    if px[3] == 0 {
      continue;
    }
    let r = px[0] as f64 / 255.0;
    let g = px[1] as f64 / 255.0;
    let b = px[2] as f64 / 255.0;
    let nr = m[0][0] * r + m[0][1] * g + m[0][2] * b + m[0][3];
    let ng = m[1][0] * r + m[1][1] * g + m[1][2] * b + m[1][3];
    let nb = m[2][0] * r + m[2][1] * g + m[2][2] * b + m[2][3];
    px[0] = (nr.clamp(0.0, 1.0) * 255.0) as u8;
    px[1] = (ng.clamp(0.0, 1.0) * 255.0) as u8;
    px[2] = (nb.clamp(0.0, 1.0) * 255.0) as u8;
  }
}

// ---- gaussian blur (3-pass box approximation) ----------------------------
//
// Three sliding-window box blurs converge to a Gaussian by the central limit
// theorem — visually indistinguishable in practice and the technique browsers
// use for canvas shadows/`filter: blur()`. Cost is O(width × height) per pass
// regardless of blur radius, vs. the kernel-sized cost of a true separable
// Gaussian.

/// In-place blur. `radius` is the canvas-spec radius in pixels; we target the
/// equivalent Gaussian (σ = radius / 2) via three box-blur passes.
pub fn gaussian_blur(buf: &mut Vec<u8>, width: u32, height: u32, radius: f64) {
  if radius <= 0.0 || width <= 1 || height <= 1 {
    return;
  }
  let sigma = radius / 2.0;
  // From "Fast Almost-Gaussian Filtering" (Wells, 2014): use three box blurs
  // with radii (m, m, m+1) where m is chosen so the variance matches σ². The
  // ideal real-valued radius is √(12σ²/n + 1)/2 with n=3 → r ≈ σ.
  let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
  let m = (ideal.floor() as i32).max(0);
  let m_odd = if m % 2 == 0 { m - 1 } else { m };
  let radii = [
    ((m_odd as f64 - 1.0) / 2.0).max(0.0) as u32,
    ((m_odd as f64 - 1.0) / 2.0).max(0.0) as u32,
    ((m_odd as f64 + 1.0) / 2.0).max(0.0) as u32,
  ];

  let mut tmp = vec![0u8; buf.len()];
  for r in radii {
    if r == 0 {
      continue;
    }
    box_blur_h(buf, &mut tmp, width, height, r);
    box_blur_v(&tmp, buf, width, height, r);
  }
}

/// Horizontal box blur: each output pixel = mean of the 2r+1 input pixels
/// horizontally centred on it. Uses an O(1)-per-pixel sliding window.
fn box_blur_h(
  src: &[u8],
  dst: &mut [u8],
  width: u32,
  height: u32,
  radius: u32,
) {
  let w = width as i32;
  let r = radius as i32;
  let win = (2 * r + 1) as u32;
  for y in 0..height as i32 {
    let row = (y * w * 4) as usize;
    let mut sum = [0u32; 4];
    // Prime the window: x in [-r, r] clamped to [0, w-1].
    for k in -r..=r {
      let sx = k.clamp(0, w - 1);
      let off = row + (sx * 4) as usize;
      for c in 0..4 {
        sum[c] += src[off + c] as u32;
      }
    }
    for x in 0..w {
      let dst_off = row + (x * 4) as usize;
      for c in 0..4 {
        dst[dst_off + c] = (sum[c] / win) as u8;
      }
      // Slide: subtract pixel leaving on the left, add pixel entering on the
      // right.
      let leave = (x - r).clamp(0, w - 1);
      let enter = (x + r + 1).clamp(0, w - 1);
      let lo = row + (leave * 4) as usize;
      let eo = row + (enter * 4) as usize;
      for c in 0..4 {
        sum[c] = sum[c] + src[eo + c] as u32 - src[lo + c] as u32;
      }
    }
  }
}

fn box_blur_v(
  src: &[u8],
  dst: &mut [u8],
  width: u32,
  height: u32,
  radius: u32,
) {
  let w = width as i32;
  let h = height as i32;
  let r = radius as i32;
  let win = (2 * r + 1) as u32;
  for x in 0..w {
    let col_byte = (x * 4) as usize;
    let mut sum = [0u32; 4];
    for k in -r..=r {
      let sy = k.clamp(0, h - 1);
      let off = (sy * w * 4) as usize + col_byte;
      for c in 0..4 {
        sum[c] += src[off + c] as u32;
      }
    }
    for y in 0..h {
      let dst_off = (y * w * 4) as usize + col_byte;
      for c in 0..4 {
        dst[dst_off + c] = (sum[c] / win) as u8;
      }
      let leave = (y - r).clamp(0, h - 1);
      let enter = (y + r + 1).clamp(0, h - 1);
      let lo = (leave * w * 4) as usize + col_byte;
      let eo = (enter * w * 4) as usize + col_byte;
      for c in 0..4 {
        sum[c] = sum[c] + src[eo + c] as u32 - src[lo + c] as u32;
      }
    }
  }
}
