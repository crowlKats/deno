// Copyright 2018-2026 the Deno authors. MIT license.
//
// Text shaping & layout for the 2d context. Uses parley for the full text
// stack (font selection via fontique, shaping via harfrust/swash, line
// breaking) and feeds the positioned glyphs to vello's `Scene::draw_glyphs`.

use std::borrow::Cow;
use std::cell::RefCell;

use parley::FontContext;
use parley::FontFeature;
use parley::FontSettings;
use parley::FontStack;
use parley::FontStyle;
use parley::FontWeight;
use parley::FontWidth;
use parley::Layout;
use parley::LayoutContext;
use parley::PositionedLayoutItem;
use parley::StyleProperty;
use vello::peniko::Font;

use super::state::CanvasFontKerning;
use super::state::CanvasFontStretch;
use super::state::CanvasFontVariantCaps;
use super::state::CanvasTextAlign;
use super::state::CanvasTextBaseline;
use super::state::CanvasTextRendering;

thread_local! {
  static FONT_CTX: RefCell<FontContext> = RefCell::new(FontContext::new());
  static LAYOUT_CTX: RefCell<LayoutContext<[u8; 4]>> =
    RefCell::new(LayoutContext::new());
}

pub struct ParsedStyle {
  pub family_source: String,
  pub size_px: f32,
  pub weight: FontWeight,
  pub style: FontStyle,
  pub width: FontWidth,
}

impl Default for ParsedStyle {
  fn default() -> Self {
    Self {
      family_source: "sans-serif".to_string(),
      size_px: 10.0,
      weight: FontWeight::NORMAL,
      style: FontStyle::Normal,
      width: FontWidth::NORMAL,
    }
  }
}

/// Parse a CSS `font` shorthand into the bits parley needs. We tokenise on
/// whitespace (respecting single/double quotes) and walk left to right:
///   1. zero or more of style / weight / stretch keywords (variant accepted
///      and discarded)
///   2. exactly one size token (a number with optional unit, optionally
///      followed by `/line-height` which we ignore)
///   3. the remainder, joined back together with spaces, becomes the family
///      list source string consumed by `FontStack::Source`
pub fn parse_font(font: &str) -> ParsedStyle {
  let mut out = ParsedStyle::default();
  let tokens = tokenise(font);
  let mut i = 0;
  let mut size_idx: Option<usize> = None;
  while i < tokens.len() {
    let tok = &tokens[i];
    if let Some((px, _line_height)) = parse_size_token(tok) {
      out.size_px = px;
      size_idx = Some(i);
      break;
    }
    if let Some(w) = parse_weight(tok) {
      out.weight = w;
    } else if let Some(s) = parse_style_kw(tok) {
      out.style = s;
    } else if let Some(width) = parse_width_kw(tok) {
      out.width = width;
    } // else: variant/normal/unknown — ignore
    i += 1;
  }
  if let Some(idx) = size_idx {
    if idx + 1 < tokens.len() {
      out.family_source = tokens[idx + 1..].join(" ");
    }
  }
  out
}

fn tokenise(s: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut buf = String::new();
  let mut quote: Option<char> = None;
  for c in s.chars() {
    if let Some(q) = quote {
      buf.push(c);
      if c == q {
        quote = None;
      }
      continue;
    }
    match c {
      '\'' | '"' => {
        quote = Some(c);
        buf.push(c);
      }
      ws if ws.is_whitespace() => {
        if !buf.is_empty() {
          out.push(std::mem::take(&mut buf));
        }
      }
      _ => buf.push(c),
    }
  }
  if !buf.is_empty() {
    out.push(buf);
  }
  out
}

fn parse_size_token(tok: &str) -> Option<(f32, Option<f32>)> {
  // Accepts "16px", "16px/20", "16/1.2", "16", "12pt".
  let (size_str, lh_str) = match tok.split_once('/') {
    Some((a, b)) => (a, Some(b)),
    None => (tok, None),
  };
  let (num, unit) = split_number_unit(size_str)?;
  let scale = match unit {
    "" | "px" => 1.0,
    "pt" => 96.0 / 72.0,
    "em" | "rem" => 16.0,
    _ => return None,
  };
  let lh = lh_str.and_then(|s| {
    let (n, _) = split_number_unit(s)?;
    Some(n as f32)
  });
  Some(((num * scale) as f32, lh))
}

fn split_number_unit(s: &str) -> Option<(f64, &str)> {
  if s.is_empty() {
    return None;
  }
  let bytes = s.as_bytes();
  let mut i = 0;
  while i < bytes.len()
    && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'-')
  {
    i += 1;
  }
  if i == 0 {
    return None;
  }
  let n = s[..i].parse::<f64>().ok()?;
  Some((n, &s[i..]))
}

fn parse_weight(s: &str) -> Option<FontWeight> {
  Some(match s {
    "normal" => FontWeight::NORMAL,
    "bold" => FontWeight::BOLD,
    "lighter" => FontWeight::EXTRA_LIGHT,
    "bolder" => FontWeight::EXTRA_BOLD,
    n if n.chars().all(|c| c.is_ascii_digit()) => {
      FontWeight::new(n.parse::<f32>().ok()?)
    }
    _ => return None,
  })
}

fn parse_style_kw(s: &str) -> Option<FontStyle> {
  Some(match s {
    "italic" => FontStyle::Italic,
    "oblique" => FontStyle::Oblique(None),
    _ => return None,
  })
}

fn parse_width_kw(s: &str) -> Option<FontWidth> {
  Some(match s {
    "ultra-condensed" => FontWidth::ULTRA_CONDENSED,
    "extra-condensed" => FontWidth::EXTRA_CONDENSED,
    "condensed" => FontWidth::CONDENSED,
    "semi-condensed" => FontWidth::SEMI_CONDENSED,
    "semi-expanded" => FontWidth::SEMI_EXPANDED,
    "expanded" => FontWidth::EXPANDED,
    "extra-expanded" => FontWidth::EXTRA_EXPANDED,
    "ultra-expanded" => FontWidth::ULTRA_EXPANDED,
    _ => return None,
  })
}

/// Extra context-state that overrides the values parsed out of the `font`
/// shorthand and / or activates OpenType features.
#[derive(Default, Clone, Copy)]
pub struct TextProps {
  pub stretch_override: Option<FontWidth>,
  pub kerning: CanvasFontKerning,
  pub variant_caps: CanvasFontVariantCaps,
  pub rendering: CanvasTextRendering,
}

impl Default for CanvasFontKerning {
  fn default() -> Self {
    CanvasFontKerning::Auto
  }
}
impl Default for CanvasFontVariantCaps {
  fn default() -> Self {
    CanvasFontVariantCaps::Normal
  }
}
impl Default for CanvasTextRendering {
  fn default() -> Self {
    CanvasTextRendering::Auto
  }
}

pub fn stretch_to_width(s: CanvasFontStretch) -> FontWidth {
  match s {
    CanvasFontStretch::UltraCondensed => FontWidth::ULTRA_CONDENSED,
    CanvasFontStretch::ExtraCondensed => FontWidth::EXTRA_CONDENSED,
    CanvasFontStretch::Condensed => FontWidth::CONDENSED,
    CanvasFontStretch::SemiCondensed => FontWidth::SEMI_CONDENSED,
    CanvasFontStretch::Normal => FontWidth::NORMAL,
    CanvasFontStretch::SemiExpanded => FontWidth::SEMI_EXPANDED,
    CanvasFontStretch::Expanded => FontWidth::EXPANDED,
    CanvasFontStretch::ExtraExpanded => FontWidth::EXTRA_EXPANDED,
    CanvasFontStretch::UltraExpanded => FontWidth::ULTRA_EXPANDED,
  }
}

fn build_features(props: TextProps) -> Vec<FontFeature> {
  let mut feats: Vec<FontFeature> = Vec::new();
  // fontKerning
  match props.kerning {
    CanvasFontKerning::None => feats.push(("kern", 0u16).into()),
    CanvasFontKerning::Normal => feats.push(("kern", 1u16).into()),
    CanvasFontKerning::Auto => {} // shaper default
  }
  // textRendering: optimizeSpeed disables kerning + ligatures
  if matches!(props.rendering, CanvasTextRendering::OptimizeSpeed) {
    feats.push(("kern", 0u16).into());
    feats.push(("liga", 0u16).into());
    feats.push(("clig", 0u16).into());
    feats.push(("calt", 0u16).into());
  }
  // fontVariantCaps
  match props.variant_caps {
    CanvasFontVariantCaps::Normal => {}
    CanvasFontVariantCaps::SmallCaps => {
      feats.push(("smcp", 1u16).into());
    }
    CanvasFontVariantCaps::AllSmallCaps => {
      feats.push(("smcp", 1u16).into());
      feats.push(("c2sc", 1u16).into());
    }
    CanvasFontVariantCaps::PetiteCaps => {
      feats.push(("pcap", 1u16).into());
    }
    CanvasFontVariantCaps::AllPetiteCaps => {
      feats.push(("pcap", 1u16).into());
      feats.push(("c2pc", 1u16).into());
    }
    CanvasFontVariantCaps::Unicase => {
      feats.push(("unic", 1u16).into());
    }
    CanvasFontVariantCaps::TitlingCaps => {
      feats.push(("titl", 1u16).into());
    }
  }
  feats
}

/// Build a parley `Layout` for `text` styled by `style` + `props`.
pub fn layout(
  text: &str,
  style: &ParsedStyle,
  props: TextProps,
  letter_spacing: f32,
  word_spacing: f32,
) -> Layout<[u8; 4]> {
  let width = props.stretch_override.unwrap_or(style.width);
  let features = build_features(props);
  FONT_CTX.with(|fctx| {
    LAYOUT_CTX.with(|lctx| {
      let mut fctx = fctx.borrow_mut();
      let mut lctx = lctx.borrow_mut();
      let mut builder = lctx.ranged_builder(&mut fctx, text, 1.0, true);
      builder.push_default(StyleProperty::FontStack(FontStack::Source(
        Cow::Borrowed(&style.family_source),
      )));
      builder.push_default(StyleProperty::FontSize(style.size_px));
      builder.push_default(StyleProperty::FontWeight(style.weight));
      builder.push_default(StyleProperty::FontStyle(style.style));
      builder.push_default(StyleProperty::FontWidth(width));
      if !features.is_empty() {
        builder.push_default(StyleProperty::FontFeatures(FontSettings::List(
          Cow::Owned(features),
        )));
      }
      if letter_spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
      }
      if word_spacing != 0.0 {
        builder.push_default(StyleProperty::WordSpacing(word_spacing));
      }
      let mut layout = builder.build(text);
      layout.break_all_lines(None);
      layout
    })
  })
}

pub struct LayoutMetrics {
  pub advance: f64,
  pub ascent: f64,
  pub descent: f64,
  pub bbox_left: f64,
  pub bbox_right: f64,
}

pub fn metrics(layout: &Layout<[u8; 4]>) -> LayoutMetrics {
  let mut ascent = 0.0f32;
  let mut descent = 0.0f32;
  let mut bbox_left = f32::INFINITY;
  let mut bbox_right = f32::NEG_INFINITY;
  let mut advance = 0.0f32;
  for line in layout.lines() {
    let m = line.metrics();
    ascent = ascent.max(m.ascent);
    descent = descent.max(m.descent);
    for item in line.items() {
      if let PositionedLayoutItem::GlyphRun(run) = item {
        let off = run.offset();
        let adv = run.advance();
        bbox_left = bbox_left.min(off);
        bbox_right = bbox_right.max(off + adv);
        advance = advance.max(off + adv);
      }
    }
  }
  if !bbox_left.is_finite() {
    bbox_left = 0.0;
  }
  if !bbox_right.is_finite() {
    bbox_right = 0.0;
  }
  LayoutMetrics {
    advance: advance as f64,
    ascent: ascent as f64,
    descent: descent as f64,
    bbox_left: bbox_left as f64,
    bbox_right: bbox_right as f64,
  }
}

/// Compute the (x, y) origin shift to apply for the requested align/baseline.
pub fn alignment_origin(
  m: &LayoutMetrics,
  align: CanvasTextAlign,
  baseline: CanvasTextBaseline,
) -> (f64, f64) {
  let dx = match align {
    CanvasTextAlign::Start | CanvasTextAlign::Left => 0.0,
    CanvasTextAlign::End | CanvasTextAlign::Right => -m.advance,
    CanvasTextAlign::Center => -m.advance / 2.0,
  };
  // parley's run baseline is measured from the top of the line (positive =
  // distance down to the baseline), so for "alphabetic" we shift the run
  // origin up by `baseline` in `draw`. Here we only return the alignment
  // delta from the requested (x, y) which is treated as the alphabetic
  // baseline by default.
  let dy = match baseline {
    CanvasTextBaseline::Alphabetic => 0.0,
    CanvasTextBaseline::Top | CanvasTextBaseline::Hanging => m.ascent,
    CanvasTextBaseline::Middle => (m.ascent - m.descent) / 2.0,
    CanvasTextBaseline::Ideographic | CanvasTextBaseline::Bottom => -m.descent,
  };
  (dx, dy)
}

/// Walk every positioned glyph in `layout` and emit a draw call per run.
/// `fillText` / `strokeText` produce a single line; glyphs come back with
/// their final layout-space `x` (advance from line start) and `y` (offset
/// from the run baseline). We emit them in coordinates relative to the
/// alphabetic baseline at x=0, and apply `scale_x` to handle `maxWidth`.
pub fn for_each_run<F>(layout: &Layout<[u8; 4]>, scale_x: f64, mut f: F)
where
  F: FnMut(&Font, f32, &mut dyn Iterator<Item = vello::Glyph>),
{
  let Some(line) = layout.lines().next() else {
    return;
  };
  for item in line.items() {
    let PositionedLayoutItem::GlyphRun(run) = item else {
      continue;
    };
    let parley_run = run.run();
    let font = parley_run.font();
    let font_size = parley_run.font_size();
    let mut iter = run.positioned_glyphs().map(move |g| vello::Glyph {
      id: g.id as u32,
      x: (g.x as f64 * scale_x) as f32,
      y: g.y,
    });
    f(font, font_size, &mut iter);
  }
}
