// Copyright 2018-2026 the Deno authors. MIT license.

use deno_core::WebIDL;
use deno_core::v8;
use vello::kurbo::Affine;
use vello::kurbo::Cap;
use vello::kurbo::Join;
use vello::kurbo::Stroke;
use vello::peniko::Brush;
use vello::peniko::Compose;
use vello::peniko::ImageQuality;
use vello::peniko::Mix;
use vello::peniko::color::AlphaColor;
use vello::peniko::color::Srgb;

pub type Color = AlphaColor<Srgb>;

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasLineCap {
  Butt,
  Round,
  Square,
}

impl From<CanvasLineCap> for Cap {
  fn from(v: CanvasLineCap) -> Self {
    match v {
      CanvasLineCap::Butt => Cap::Butt,
      CanvasLineCap::Round => Cap::Round,
      CanvasLineCap::Square => Cap::Square,
    }
  }
}

impl From<Cap> for CanvasLineCap {
  fn from(v: Cap) -> Self {
    match v {
      Cap::Butt => CanvasLineCap::Butt,
      Cap::Round => CanvasLineCap::Round,
      Cap::Square => CanvasLineCap::Square,
    }
  }
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasLineJoin {
  Bevel,
  Round,
  Miter,
}

impl From<CanvasLineJoin> for Join {
  fn from(v: CanvasLineJoin) -> Self {
    match v {
      CanvasLineJoin::Bevel => Join::Bevel,
      CanvasLineJoin::Round => Join::Round,
      CanvasLineJoin::Miter => Join::Miter,
    }
  }
}

impl From<Join> for CanvasLineJoin {
  fn from(v: Join) -> Self {
    match v {
      Join::Bevel => CanvasLineJoin::Bevel,
      Join::Round => CanvasLineJoin::Round,
      Join::Miter => CanvasLineJoin::Miter,
    }
  }
}

#[derive(WebIDL, Clone, Copy, PartialEq, Eq)]
#[webidl(enum)]
pub enum CanvasFillRule {
  Nonzero,
  Evenodd,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasTextAlign {
  Start,
  End,
  Left,
  Right,
  Center,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasTextBaseline {
  Top,
  Hanging,
  Middle,
  Alphabetic,
  Ideographic,
  Bottom,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasDirection {
  Ltr,
  Rtl,
  Inherit,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasFontKerning {
  Auto,
  Normal,
  None,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
#[allow(non_camel_case_types)]
pub enum CanvasFontStretch {
  UltraCondensed,
  ExtraCondensed,
  Condensed,
  SemiCondensed,
  Normal,
  SemiExpanded,
  Expanded,
  ExtraExpanded,
  UltraExpanded,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasFontVariantCaps {
  Normal,
  SmallCaps,
  AllSmallCaps,
  PetiteCaps,
  AllPetiteCaps,
  Unicase,
  TitlingCaps,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum CanvasTextRendering {
  Auto,
  OptimizeSpeed,
  OptimizeLegibility,
  GeometricPrecision,
}

#[derive(WebIDL, Clone, Copy)]
#[webidl(enum)]
pub enum ImageSmoothingQuality {
  Low,
  Medium,
  High,
}

impl From<ImageSmoothingQuality> for ImageQuality {
  fn from(v: ImageSmoothingQuality) -> Self {
    match v {
      ImageSmoothingQuality::Low => ImageQuality::Low,
      ImageSmoothingQuality::Medium => ImageQuality::Medium,
      ImageSmoothingQuality::High => ImageQuality::High,
    }
  }
}

#[derive(WebIDL, Clone, Copy, PartialEq, Eq)]
#[webidl(enum)]
pub enum GlobalCompositeOperation {
  SourceOver,
  SourceIn,
  SourceOut,
  SourceAtop,
  DestinationOver,
  DestinationIn,
  DestinationOut,
  DestinationAtop,
  Lighter,
  Copy,
  Xor,
  Multiply,
  Screen,
  Overlay,
  Darken,
  Lighten,
  ColorDodge,
  ColorBurn,
  HardLight,
  SoftLight,
  Difference,
  Exclusion,
  Hue,
  Saturation,
  Color,
  Luminosity,
}

impl GlobalCompositeOperation {
  /// Map to (Mix, Compose). Many entries collapse to peniko equivalents;
  /// pure compositing modes use Mix::Normal with a Compose mode, blending
  /// modes use the matching Mix with Compose::SrcOver.
  pub fn to_blend(self) -> (Mix, Compose) {
    use GlobalCompositeOperation::*;
    match self {
      SourceOver => (Mix::Normal, Compose::SrcOver),
      SourceIn => (Mix::Normal, Compose::SrcIn),
      SourceOut => (Mix::Normal, Compose::SrcOut),
      SourceAtop => (Mix::Normal, Compose::SrcAtop),
      DestinationOver => (Mix::Normal, Compose::DestOver),
      DestinationIn => (Mix::Normal, Compose::DestIn),
      DestinationOut => (Mix::Normal, Compose::DestOut),
      DestinationAtop => (Mix::Normal, Compose::DestAtop),
      Lighter => (Mix::Normal, Compose::Plus),
      Copy => (Mix::Normal, Compose::Copy),
      Xor => (Mix::Normal, Compose::Xor),
      Multiply => (Mix::Multiply, Compose::SrcOver),
      Screen => (Mix::Screen, Compose::SrcOver),
      Overlay => (Mix::Overlay, Compose::SrcOver),
      Darken => (Mix::Darken, Compose::SrcOver),
      Lighten => (Mix::Lighten, Compose::SrcOver),
      ColorDodge => (Mix::ColorDodge, Compose::SrcOver),
      ColorBurn => (Mix::ColorBurn, Compose::SrcOver),
      HardLight => (Mix::HardLight, Compose::SrcOver),
      SoftLight => (Mix::SoftLight, Compose::SrcOver),
      Difference => (Mix::Difference, Compose::SrcOver),
      Exclusion => (Mix::Exclusion, Compose::SrcOver),
      Hue => (Mix::Hue, Compose::SrcOver),
      Saturation => (Mix::Saturation, Compose::SrcOver),
      Color => (Mix::Color, Compose::SrcOver),
      Luminosity => (Mix::Luminosity, Compose::SrcOver),
    }
  }
}

/// Brush slot for fillStyle / strokeStyle. Holds the peniko brush plus an
/// optional v8 reference for round-tripping CanvasGradient / CanvasPattern
/// through the getter.
#[derive(Clone)]
pub enum StyleBrush {
  Solid(Color),
  Object {
    brush: Brush,
    /// Cached v8 reference (CanvasGradient or CanvasPattern instance).
    object: v8::Global<v8::Object>,
  },
}

impl StyleBrush {
  pub fn brush(&self) -> Brush {
    match self {
      StyleBrush::Solid(c) => Brush::Solid(*c),
      StyleBrush::Object { brush, .. } => brush.clone(),
    }
  }
}

#[derive(Clone)]
pub struct DrawState {
  pub transform: Affine,
  pub fill_style: StyleBrush,
  pub stroke_style: StyleBrush,
  pub line_width: f64,
  pub line_cap: Cap,
  pub line_join: Join,
  pub miter_limit: f64,
  pub line_dash: Vec<f64>,
  pub line_dash_offset: f64,
  pub global_alpha: f32,
  pub global_composite_operation: GlobalCompositeOperation,
  pub filter: String,
  pub image_smoothing_enabled: bool,
  pub image_smoothing_quality: ImageSmoothingQuality,
  pub shadow_color: Color,
  pub shadow_blur: f64,
  pub shadow_offset_x: f64,
  pub shadow_offset_y: f64,
  pub font: String,
  pub text_align: CanvasTextAlign,
  pub text_baseline: CanvasTextBaseline,
  pub direction: CanvasDirection,
  pub letter_spacing: String,
  pub word_spacing: String,
  pub font_kerning: CanvasFontKerning,
  pub font_stretch: CanvasFontStretch,
  pub font_variant_caps: CanvasFontVariantCaps,
  pub text_rendering: CanvasTextRendering,
  /// Active clip path (in user space at the time of `clip()`), if any.
  pub clip: Option<(vello::kurbo::BezPath, CanvasFillRule, Affine)>,
}

impl Default for DrawState {
  fn default() -> Self {
    Self {
      transform: Affine::IDENTITY,
      fill_style: StyleBrush::Solid(Color::BLACK),
      stroke_style: StyleBrush::Solid(Color::BLACK),
      line_width: 1.0,
      line_cap: Cap::Butt,
      line_join: Join::Miter,
      miter_limit: 10.0,
      line_dash: Vec::new(),
      line_dash_offset: 0.0,
      global_alpha: 1.0,
      global_composite_operation: GlobalCompositeOperation::SourceOver,
      filter: String::from("none"),
      image_smoothing_enabled: true,
      image_smoothing_quality: ImageSmoothingQuality::Low,
      shadow_color: Color::TRANSPARENT,
      shadow_blur: 0.0,
      shadow_offset_x: 0.0,
      shadow_offset_y: 0.0,
      font: String::from("10px sans-serif"),
      text_align: CanvasTextAlign::Start,
      text_baseline: CanvasTextBaseline::Alphabetic,
      direction: CanvasDirection::Inherit,
      letter_spacing: String::from("0px"),
      word_spacing: String::from("0px"),
      font_kerning: CanvasFontKerning::Auto,
      font_stretch: CanvasFontStretch::Normal,
      font_variant_caps: CanvasFontVariantCaps::Normal,
      text_rendering: CanvasTextRendering::Auto,
      clip: None,
    }
  }
}

impl DrawState {
  pub fn stroke(&self) -> Stroke {
    let mut s = Stroke::new(self.line_width)
      .with_caps(self.line_cap)
      .with_join(self.line_join)
      .with_miter_limit(self.miter_limit);
    if !self.line_dash.is_empty() {
      s = s.with_dashes(self.line_dash_offset, self.line_dash.iter().copied());
    }
    s
  }
}

/// Parse a CSS-ish color string. Supports `#rgb`, `#rrggbb`, `#rrggbbaa`,
/// `rgb()`, `rgba()`, and a handful of named colors. Returns None on unknown.
pub fn parse_color(s: &str) -> Option<Color> {
  let s = s.trim();
  if let Some(hex) = s.strip_prefix('#') {
    return parse_hex(hex);
  }
  if let Some(inner) = s
    .strip_prefix("rgba(")
    .or_else(|| s.strip_prefix("rgb("))
    .and_then(|x| x.strip_suffix(')'))
  {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() == 3 || parts.len() == 4 {
      let r = parts[0].parse::<f32>().ok()?;
      let g = parts[1].parse::<f32>().ok()?;
      let b = parts[2].parse::<f32>().ok()?;
      let a = parts
        .get(3)
        .and_then(|p| p.parse::<f32>().ok())
        .unwrap_or(1.0);
      return Some(Color::from_rgba8(
        r.clamp(0.0, 255.0) as u8,
        g.clamp(0.0, 255.0) as u8,
        b.clamp(0.0, 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
      ));
    }
    return None;
  }
  match s.to_ascii_lowercase().as_str() {
    "black" => Some(Color::BLACK),
    "white" => Some(Color::WHITE),
    "red" => Some(Color::from_rgba8(255, 0, 0, 255)),
    "green" => Some(Color::from_rgba8(0, 128, 0, 255)),
    "blue" => Some(Color::from_rgba8(0, 0, 255, 255)),
    "yellow" => Some(Color::from_rgba8(255, 255, 0, 255)),
    "cyan" | "aqua" => Some(Color::from_rgba8(0, 255, 255, 255)),
    "magenta" | "fuchsia" => Some(Color::from_rgba8(255, 0, 255, 255)),
    "gray" | "grey" => Some(Color::from_rgba8(128, 128, 128, 255)),
    "silver" => Some(Color::from_rgba8(192, 192, 192, 255)),
    "transparent" => Some(Color::TRANSPARENT),
    _ => None,
  }
}

fn parse_hex(hex: &str) -> Option<Color> {
  match hex.len() {
    3 => {
      let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 0x11;
      let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 0x11;
      let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 0x11;
      Some(Color::from_rgba8(r, g, b, 255))
    }
    4 => {
      let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 0x11;
      let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 0x11;
      let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 0x11;
      let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 0x11;
      Some(Color::from_rgba8(r, g, b, a))
    }
    6 => {
      let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
      Some(Color::from_rgba8(r, g, b, 255))
    }
    8 => {
      let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
      let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
      Some(Color::from_rgba8(r, g, b, a))
    }
    _ => None,
  }
}

pub fn color_to_string(c: Color) -> String {
  let rgba = c.to_rgba8().to_u8_array();
  if rgba[3] == 255 {
    format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
  } else {
    format!(
      "rgba({}, {}, {}, {})",
      rgba[0],
      rgba[1],
      rgba[2],
      (rgba[3] as f32) / 255.0
    )
  }
}
