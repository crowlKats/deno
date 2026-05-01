// Copyright 2018-2026 the Deno authors. MIT license.
//
// CanvasRenderingContext2D — vello-backed implementation. Surface-backed
// canvases are not yet supported. Text uses parley for the full stack:
// fontique for system font selection from the CSS `font-family` list,
// harfrust shaping (kerning, ligatures, RTL, complex scripts), and the
// resulting glyph runs feed vello's `Scene::draw_glyphs`.

use std::cell::RefCell;
use std::rc::Rc;

use deno_core::GarbageCollected;
use deno_core::WebIDL;
use deno_core::cppgc::Ref;
use deno_core::op2;
use deno_core::v8;
use deno_core::v8::cppgc::Visitor;
use deno_core::webidl::UnrestrictedDouble;
use deno_error::JsErrorBox;
use deno_image::bitmap::ImageBitmap;
use deno_image::image;
use deno_image::image::DynamicImage;
use deno_image::image::GenericImageView;
use deno_image::image::RgbaImage;
use deno_webgpu::canvas::ContextData;
use vello::Scene;
use vello::kurbo::Affine;
use vello::kurbo::BezPath;
use vello::kurbo::Point;
use vello::kurbo::Rect;
use vello::kurbo::Shape;
use vello::peniko::BlendMode;
use vello::peniko::Blob;
use vello::peniko::Brush;
use vello::peniko::Fill;
use vello::peniko::Image;
use vello::peniko::ImageFormat;

mod effects;
mod gradient;
mod image_data;
mod path;
mod pattern;
mod renderer;
mod state;
mod text;
mod text_metrics;

pub use gradient::CanvasGradient;
pub use image_data::ImageData;
pub use image_data::PredefinedColorSpace;
pub use path::Path2D;
use path::PathBuilder;
use path::parse_dom_matrix_init;
use path::parse_round_rect_radii;
use path::point_in_path;
pub use pattern::CanvasPattern;
use pattern::PatternRepetition;
use state::CanvasDirection;
use state::CanvasFillRule;
use state::CanvasFontKerning;
use state::CanvasFontStretch;
use state::CanvasFontVariantCaps;
use state::CanvasLineCap;
use state::CanvasLineJoin;
use state::CanvasTextAlign;
use state::CanvasTextBaseline;
use state::CanvasTextRendering;
use state::Color;
use state::DrawState;
use state::GlobalCompositeOperation;
use state::ImageSmoothingQuality;
use state::StyleBrush;
use state::color_to_string;
use state::parse_color;
pub use text_metrics::TextMetrics;

pub const CONTEXT_ID: &str = "2d";

pub struct CanvasRenderingContext2D {
  canvas: v8::Global<v8::Object>,
  /// CPU-side backing bitmap used by every drawing op. For
  /// `ContextData::Canvas`, this is the same `Rc` the canvas itself owns. For
  /// `ContextData::Surface`, we own it independently and present it to the
  /// surface via `surface_only` on `present()`.
  bitmap: Rc<RefCell<DynamicImage>>,
  data: ContextData,
  /// wgpu pipeline used to push the bitmap to a `ContextData::Surface`. None
  /// for canvas-backed contexts.
  pub(crate) surface_only: Option<crate::bitmaprenderer::SurfaceBitmap>,
  alpha: bool,
  inner: RefCell<Inner>,
}

struct Inner {
  state_stack: Vec<DrawState>,
  path: PathBuilder,
}

impl Inner {
  fn cur(&self) -> &DrawState {
    self.state_stack.last().unwrap()
  }
  fn cur_mut(&mut self) -> &mut DrawState {
    self.state_stack.last_mut().unwrap()
  }
}

// SAFETY: held v8::Global handles are strong roots; no extra tracing required.
unsafe impl GarbageCollected for CanvasRenderingContext2D {
  fn trace(&self, _visitor: &mut Visitor) {}
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"CanvasRenderingContext2D"
  }
}

#[op2]
impl CanvasRenderingContext2D {
  #[getter]
  fn canvas(&self) -> v8::Global<v8::Object> {
    self.canvas.clone()
  }

  fn get_context_attributes<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> v8::Local<'s, v8::Object> {
    let null = v8::null(scope);
    let obj = v8::Object::with_prototype_and_properties(
      scope,
      null.into(),
      &[v8::String::new(scope, "alpha").unwrap().into()],
      &[v8::Boolean::new(scope, self.alpha).into()],
    );
    obj
  }

  #[getter]
  fn is_context_lost(&self) -> bool {
    false
  }

  // -- state stack --------------------------------------------------------

  #[fast]
  fn save(&self) {
    let mut inner = self.inner.borrow_mut();
    let snap = inner.cur().clone();
    inner.state_stack.push(snap);
  }

  #[fast]
  fn restore(&self) {
    let mut inner = self.inner.borrow_mut();
    if inner.state_stack.len() > 1 {
      inner.state_stack.pop();
    }
  }

  #[fast]
  fn reset(&self) -> Result<(), JsErrorBox> {
    {
      let mut inner = self.inner.borrow_mut();
      inner.state_stack.clear();
      inner.state_stack.push(DrawState::default());
      inner.path.reset();
    }
    self.clear_bitmap()
  }

  // -- styles -------------------------------------------------------------

  #[getter]
  fn fill_style<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> v8::Local<'s, v8::Value> {
    style_to_v8(scope, &self.inner.borrow().cur().fill_style)
  }
  #[setter]
  fn fill_style<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
  ) {
    if let Some(s) = parse_style(scope, value) {
      self.inner.borrow_mut().cur_mut().fill_style = s;
    }
  }

  #[getter]
  fn stroke_style<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> v8::Local<'s, v8::Value> {
    style_to_v8(scope, &self.inner.borrow().cur().stroke_style)
  }
  #[setter]
  fn stroke_style<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
  ) {
    if let Some(s) = parse_style(scope, value) {
      self.inner.borrow_mut().cur_mut().stroke_style = s;
    }
  }

  #[getter]
  fn line_width(&self) -> f64 {
    self.inner.borrow().cur().line_width
  }
  #[setter]
  fn line_width(&self, #[webidl] value: UnrestrictedDouble) {
    let v = *value;
    if v > 0.0 && v.is_finite() {
      self.inner.borrow_mut().cur_mut().line_width = v;
    }
  }

  #[getter]
  #[string]
  fn line_cap(&self) -> &'static str {
    CanvasLineCap::from(self.inner.borrow().cur().line_cap).as_str()
  }
  #[setter]
  fn line_cap(&self, #[webidl] value: CanvasLineCap) {
    self.inner.borrow_mut().cur_mut().line_cap = value.into();
  }

  #[getter]
  #[string]
  fn line_join(&self) -> &'static str {
    CanvasLineJoin::from(self.inner.borrow().cur().line_join).as_str()
  }
  #[setter]
  fn line_join(&self, #[webidl] value: CanvasLineJoin) {
    self.inner.borrow_mut().cur_mut().line_join = value.into();
  }

  #[getter]
  fn miter_limit(&self) -> f64 {
    self.inner.borrow().cur().miter_limit
  }
  #[setter]
  fn miter_limit(&self, #[webidl] value: UnrestrictedDouble) {
    let v = *value;
    if v > 0.0 && v.is_finite() {
      self.inner.borrow_mut().cur_mut().miter_limit = v;
    }
  }

  #[getter]
  fn line_dash_offset(&self) -> f64 {
    self.inner.borrow().cur().line_dash_offset
  }
  #[setter]
  fn line_dash_offset(&self, #[webidl] value: UnrestrictedDouble) {
    let v = *value;
    if v.is_finite() {
      self.inner.borrow_mut().cur_mut().line_dash_offset = v;
    }
  }

  fn set_line_dash(&self, #[webidl] segments: Vec<UnrestrictedDouble>) {
    // Per spec: if any value in segments is not finite or negative, return
    // without throwing.
    if segments.iter().any(|v| !v.is_finite() || **v < 0.0) {
      return;
    }
    let mut out: Vec<f64> = segments.into_iter().map(|v| *v).collect();
    if out.len() % 2 != 0 {
      out.extend_from_slice(&out.clone());
    }
    self.inner.borrow_mut().cur_mut().line_dash = out;
  }

  fn get_line_dash<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> v8::Local<'s, v8::Array> {
    let dash = self.inner.borrow().cur().line_dash.clone();
    let elems: Vec<v8::Local<v8::Value>> = dash
      .iter()
      .map(|n| v8::Number::new(scope, *n).into())
      .collect();
    v8::Array::new_with_elements(scope, &elems)
  }

  #[getter]
  fn global_alpha(&self) -> f64 {
    self.inner.borrow().cur().global_alpha as f64
  }
  #[setter]
  fn global_alpha(&self, #[webidl] value: UnrestrictedDouble) {
    let v = *value;
    if v.is_finite() && (0.0..=1.0).contains(&v) {
      self.inner.borrow_mut().cur_mut().global_alpha = v as f32;
    }
  }

  #[getter]
  #[string]
  fn global_composite_operation(&self) -> &'static str {
    self
      .inner
      .borrow()
      .cur()
      .global_composite_operation
      .as_str()
  }
  #[setter]
  fn global_composite_operation(
    &self,
    #[webidl] value: GlobalCompositeOperation,
  ) {
    self.inner.borrow_mut().cur_mut().global_composite_operation = value;
  }

  #[getter]
  #[string]
  fn filter(&self) -> String {
    self.inner.borrow().cur().filter.clone()
  }
  #[setter]
  fn filter(&self, #[webidl] value: String) {
    self.inner.borrow_mut().cur_mut().filter = value;
  }

  #[getter]
  fn image_smoothing_enabled(&self) -> bool {
    self.inner.borrow().cur().image_smoothing_enabled
  }
  #[setter]
  fn image_smoothing_enabled(&self, value: bool) {
    self.inner.borrow_mut().cur_mut().image_smoothing_enabled = value;
  }

  #[getter]
  #[string]
  fn image_smoothing_quality(&self) -> &'static str {
    self.inner.borrow().cur().image_smoothing_quality.as_str()
  }
  #[setter]
  fn image_smoothing_quality(&self, #[webidl] value: ImageSmoothingQuality) {
    self.inner.borrow_mut().cur_mut().image_smoothing_quality = value;
  }

  // -- shadows (state only; no rendering yet) ------------------------------

  #[getter]
  #[string]
  fn shadow_color(&self) -> String {
    color_to_string(self.inner.borrow().cur().shadow_color)
  }
  #[setter]
  fn shadow_color(&self, #[webidl] value: String) {
    if let Some(c) = parse_color(&value) {
      self.inner.borrow_mut().cur_mut().shadow_color = c;
    }
  }

  #[getter]
  fn shadow_blur(&self) -> f64 {
    self.inner.borrow().cur().shadow_blur
  }
  #[setter]
  fn shadow_blur(&self, #[webidl] value: UnrestrictedDouble) {
    let v = *value;
    if v.is_finite() && v >= 0.0 {
      self.inner.borrow_mut().cur_mut().shadow_blur = v;
    }
  }

  #[getter]
  fn shadow_offset_x(&self) -> f64 {
    self.inner.borrow().cur().shadow_offset_x
  }
  #[setter]
  fn shadow_offset_x(&self, #[webidl] value: UnrestrictedDouble) {
    if value.is_finite() {
      self.inner.borrow_mut().cur_mut().shadow_offset_x = *value;
    }
  }

  #[getter]
  fn shadow_offset_y(&self) -> f64 {
    self.inner.borrow().cur().shadow_offset_y
  }
  #[setter]
  fn shadow_offset_y(&self, #[webidl] value: UnrestrictedDouble) {
    if value.is_finite() {
      self.inner.borrow_mut().cur_mut().shadow_offset_y = *value;
    }
  }

  // -- text state (rendering not yet implemented) --------------------------

  #[getter]
  #[string]
  fn font(&self) -> String {
    self.inner.borrow().cur().font.clone()
  }
  #[setter]
  fn font(&self, #[webidl] value: String) {
    self.inner.borrow_mut().cur_mut().font = value;
  }

  #[getter]
  #[string]
  fn text_align(&self) -> &'static str {
    self.inner.borrow().cur().text_align.as_str()
  }
  #[setter]
  fn text_align(&self, #[webidl] value: CanvasTextAlign) {
    self.inner.borrow_mut().cur_mut().text_align = value;
  }

  #[getter]
  #[string]
  fn text_baseline(&self) -> &'static str {
    self.inner.borrow().cur().text_baseline.as_str()
  }
  #[setter]
  fn text_baseline(&self, #[webidl] value: CanvasTextBaseline) {
    self.inner.borrow_mut().cur_mut().text_baseline = value;
  }

  #[getter]
  #[string]
  fn direction(&self) -> &'static str {
    self.inner.borrow().cur().direction.as_str()
  }
  #[setter]
  fn direction(&self, #[webidl] value: CanvasDirection) {
    self.inner.borrow_mut().cur_mut().direction = value;
  }

  #[getter]
  #[string]
  fn letter_spacing(&self) -> String {
    self.inner.borrow().cur().letter_spacing.clone()
  }
  #[setter]
  fn letter_spacing(&self, #[webidl] value: String) {
    self.inner.borrow_mut().cur_mut().letter_spacing = value;
  }

  #[getter]
  #[string]
  fn word_spacing(&self) -> String {
    self.inner.borrow().cur().word_spacing.clone()
  }
  #[setter]
  fn word_spacing(&self, #[webidl] value: String) {
    self.inner.borrow_mut().cur_mut().word_spacing = value;
  }

  #[getter]
  #[string]
  fn font_kerning(&self) -> &'static str {
    self.inner.borrow().cur().font_kerning.as_str()
  }
  #[setter]
  fn font_kerning(&self, #[webidl] value: CanvasFontKerning) {
    self.inner.borrow_mut().cur_mut().font_kerning = value;
  }

  #[getter]
  #[string]
  fn font_stretch(&self) -> &'static str {
    self.inner.borrow().cur().font_stretch.as_str()
  }
  #[setter]
  fn font_stretch(&self, #[webidl] value: CanvasFontStretch) {
    self.inner.borrow_mut().cur_mut().font_stretch = value;
  }

  #[getter]
  #[string]
  fn font_variant_caps(&self) -> &'static str {
    self.inner.borrow().cur().font_variant_caps.as_str()
  }
  #[setter]
  fn font_variant_caps(&self, #[webidl] value: CanvasFontVariantCaps) {
    self.inner.borrow_mut().cur_mut().font_variant_caps = value;
  }

  #[getter]
  #[string]
  fn text_rendering(&self) -> &'static str {
    self.inner.borrow().cur().text_rendering.as_str()
  }
  #[setter]
  fn text_rendering(&self, #[webidl] value: CanvasTextRendering) {
    self.inner.borrow_mut().cur_mut().text_rendering = value;
  }

  // -- transforms ---------------------------------------------------------

  fn translate(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    let mut inner = self.inner.borrow_mut();
    let t = inner.cur().transform;
    inner.cur_mut().transform = t * Affine::translate((*x, *y));
  }

  fn scale(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    let mut inner = self.inner.borrow_mut();
    let t = inner.cur().transform;
    inner.cur_mut().transform = t * Affine::scale_non_uniform(*x, *y);
  }

  fn rotate(&self, #[webidl] angle: UnrestrictedDouble) {
    let mut inner = self.inner.borrow_mut();
    let t = inner.cur().transform;
    inner.cur_mut().transform = t * Affine::rotate(*angle);
  }

  fn transform(
    &self,
    #[webidl] a: UnrestrictedDouble,
    #[webidl] b: UnrestrictedDouble,
    #[webidl] c: UnrestrictedDouble,
    #[webidl] d: UnrestrictedDouble,
    #[webidl] e: UnrestrictedDouble,
    #[webidl] f: UnrestrictedDouble,
  ) {
    let mut inner = self.inner.borrow_mut();
    let t = inner.cur().transform;
    inner.cur_mut().transform = t * Affine::new([*a, *b, *c, *d, *e, *f]);
  }

  #[nofast]
  #[reentrant]
  fn set_transform<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
    arg3: v8::Local<'s, v8::Value>,
    arg4: v8::Local<'s, v8::Value>,
    arg5: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    if arg0.is_object() && arg1.is_undefined() {
      let t = parse_dom_matrix_init(scope, arg0)?;
      self.inner.borrow_mut().cur_mut().transform = t;
      return Ok(());
    }
    let n = |v: v8::Local<'s, v8::Value>| v.number_value(scope).unwrap_or(0.0);
    self.inner.borrow_mut().cur_mut().transform =
      Affine::new([n(arg0), n(arg1), n(arg2), n(arg3), n(arg4), n(arg5)]);
    Ok(())
  }

  #[fast]
  fn reset_transform(&self) {
    self.inner.borrow_mut().cur_mut().transform = Affine::IDENTITY;
  }

  #[cppgc]
  fn get_transform(&self) -> deno_web::DOMMatrix {
    let [a, b, c, d, e, f] = self.inner.borrow().cur().transform.as_coeffs();
    deno_web::DOMMatrix::from_2d_coefficients(a, b, c, d, e, f)
  }

  // -- path ---------------------------------------------------------------

  #[fast]
  fn begin_path(&self) {
    self.inner.borrow_mut().path.reset();
  }

  #[fast]
  fn close_path(&self) {
    self.inner.borrow_mut().path.close_path();
  }

  fn move_to(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.inner.borrow_mut().path.move_to(*x, *y);
  }

  fn line_to(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.inner.borrow_mut().path.line_to(*x, *y);
  }

  fn bezier_curve_to(
    &self,
    #[webidl] cp1x: UnrestrictedDouble,
    #[webidl] cp1y: UnrestrictedDouble,
    #[webidl] cp2x: UnrestrictedDouble,
    #[webidl] cp2y: UnrestrictedDouble,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self
      .inner
      .borrow_mut()
      .path
      .bezier_to(*cp1x, *cp1y, *cp2x, *cp2y, *x, *y);
  }

  fn quadratic_curve_to(
    &self,
    #[webidl] cpx: UnrestrictedDouble,
    #[webidl] cpy: UnrestrictedDouble,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.inner.borrow_mut().path.quad_to(*cpx, *cpy, *x, *y);
  }

  fn rect(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
  ) {
    self.inner.borrow_mut().path.rect(*x, *y, *w, *h);
  }

  #[reentrant]
  fn round_rect<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
    radii: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let rr = parse_round_rect_radii(scope, radii)?;
    self.inner.borrow_mut().path.round_rect(*x, *y, *w, *h, rr);
    Ok(())
  }

  fn arc(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] radius: UnrestrictedDouble,
    #[webidl] start_angle: UnrestrictedDouble,
    #[webidl] end_angle: UnrestrictedDouble,
    #[webidl] counterclockwise: Option<bool>,
  ) -> Result<(), JsErrorBox> {
    self.inner.borrow_mut().path.arc(
      *x,
      *y,
      *radius,
      *start_angle,
      *end_angle,
      counterclockwise.unwrap_or(false),
    )
  }

  fn arc_to(
    &self,
    #[webidl] x1: UnrestrictedDouble,
    #[webidl] y1: UnrestrictedDouble,
    #[webidl] x2: UnrestrictedDouble,
    #[webidl] y2: UnrestrictedDouble,
    #[webidl] radius: UnrestrictedDouble,
  ) -> Result<(), JsErrorBox> {
    self
      .inner
      .borrow_mut()
      .path
      .arc_to(*x1, *y1, *x2, *y2, *radius)
  }

  #[allow(clippy::too_many_arguments)]
  fn ellipse(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] radius_x: UnrestrictedDouble,
    #[webidl] radius_y: UnrestrictedDouble,
    #[webidl] rotation: UnrestrictedDouble,
    #[webidl] start_angle: UnrestrictedDouble,
    #[webidl] end_angle: UnrestrictedDouble,
    #[webidl] counterclockwise: Option<bool>,
  ) -> Result<(), JsErrorBox> {
    self.inner.borrow_mut().path.ellipse(
      *x,
      *y,
      *radius_x,
      *radius_y,
      *rotation,
      *start_angle,
      *end_angle,
      counterclockwise.unwrap_or(false),
    )
  }

  // -- drawing ------------------------------------------------------------

  #[fast]
  fn fill<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let (path, rule) = resolve_path_args(scope, arg0, arg1)?;
    let path = path.unwrap_or_else(|| self.inner.borrow().path.bez.clone());
    let (transform, brush, brush_t, alpha, blend, clip) = self.draw_state();
    let fill_kind = match rule {
      CanvasFillRule::Nonzero => Fill::NonZero,
      CanvasFillRule::Evenodd => Fill::EvenOdd,
    };
    self.flush_with(blend, clip, |scene| {
      scene.fill(
        fill_kind,
        transform,
        &alpha_brush(&brush, alpha),
        brush_t,
        &path,
      );
    })
  }

  #[fast]
  fn stroke<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let path = if let Some(p) = unwrap_path2d(scope, arg0) {
      p.builder.borrow().bez.clone()
    } else {
      self.inner.borrow().path.bez.clone()
    };
    let (transform, _, _, alpha, blend, clip) = self.draw_state();
    let (brush, brush_t) = self.stroke_brush();
    let stroke = self.inner.borrow().cur().stroke();
    self.flush_with(blend, clip, |scene| {
      scene.stroke(
        &stroke,
        transform,
        &alpha_brush(&brush, alpha),
        brush_t,
        &path,
      );
    })
  }

  #[fast]
  fn clip<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let (path, rule) = resolve_path_args(scope, arg0, arg1)?;
    let path = path.unwrap_or_else(|| self.inner.borrow().path.bez.clone());
    let mut inner = self.inner.borrow_mut();
    let t = inner.cur().transform;
    let combined = match inner.cur().clip.clone() {
      Some((mut existing, _, _)) => {
        existing.extend(path.iter());
        existing
      }
      None => path,
    };
    inner.cur_mut().clip = Some((combined, rule, t));
    Ok(())
  }

  #[fast]
  fn is_point_in_path<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
    arg3: v8::Local<'s, v8::Value>,
  ) -> Result<bool, JsErrorBox> {
    // Forms: (x, y[, fillRule]) or (path, x, y[, fillRule]).
    let (path, x, y, rule) = if let Some(p) = unwrap_path2d(scope, arg0) {
      let x = arg1.number_value(scope).unwrap_or(f64::NAN);
      let y = arg2.number_value(scope).unwrap_or(f64::NAN);
      let rule = parse_fill_rule(scope, arg3)?;
      (p.builder.borrow().bez.clone(), x, y, rule)
    } else {
      let x = arg0.number_value(scope).unwrap_or(f64::NAN);
      let y = arg1.number_value(scope).unwrap_or(f64::NAN);
      let rule = parse_fill_rule(scope, arg2)?;
      (self.inner.borrow().path.bez.clone(), x, y, rule)
    };
    if !x.is_finite() || !y.is_finite() {
      return Ok(false);
    }
    let transform = self.inner.borrow().cur().transform;
    let inv = transform.inverse();
    let local = inv * Point::new(x, y);
    Ok(point_in_path(&path, local, rule))
  }

  #[fast]
  fn is_point_in_stroke<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
  ) -> Result<bool, JsErrorBox> {
    let (path, x, y) = if let Some(p) = unwrap_path2d(scope, arg0) {
      let x = arg1.number_value(scope).unwrap_or(f64::NAN);
      let y = arg2.number_value(scope).unwrap_or(f64::NAN);
      (p.builder.borrow().bez.clone(), x, y)
    } else {
      let x = arg0.number_value(scope).unwrap_or(f64::NAN);
      let y = arg1.number_value(scope).unwrap_or(f64::NAN);
      (self.inner.borrow().path.bez.clone(), x, y)
    };
    if !x.is_finite() || !y.is_finite() {
      return Ok(false);
    }
    let transform = self.inner.borrow().cur().transform;
    let inv = transform.inverse();
    let local = inv * Point::new(x, y);
    let stroke = self.inner.borrow().cur().stroke();
    let stroked = vello::kurbo::stroke(
      path.path_elements(0.1),
      &stroke,
      &Default::default(),
      0.1,
    );
    Ok(point_in_path(&stroked, local, CanvasFillRule::Nonzero))
  }

  fn fill_rect(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
  ) -> Result<(), JsErrorBox> {
    let (transform, brush, brush_t, alpha, blend, clip) = self.draw_state();
    let rect = Rect::new(*x, *y, *x + *w, *y + *h);
    self.flush_with(blend, clip, |scene| {
      scene.fill(
        Fill::NonZero,
        transform,
        &alpha_brush(&brush, alpha),
        brush_t,
        &rect,
      );
    })
  }

  fn stroke_rect(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
  ) -> Result<(), JsErrorBox> {
    let (transform, _, _, alpha, blend, clip) = self.draw_state();
    let (brush, brush_t) = self.stroke_brush();
    let stroke = self.inner.borrow().cur().stroke();
    let rect = Rect::new(*x, *y, *x + *w, *y + *h);
    self.flush_with(blend, clip, |scene| {
      scene.stroke(
        &stroke,
        transform,
        &alpha_brush(&brush, alpha),
        brush_t,
        &rect,
      );
    })
  }

  fn clear_rect(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
  ) -> Result<(), JsErrorBox> {
    let (x, y, w, h) = (*x, *y, *w, *h);
    let transform = self.inner.borrow().cur().transform;
    let image = self.canvas_data()?;
    let (cw, ch) = image.borrow().dimensions();
    let rect = Rect::new(x, y, x + w, y + h);
    let bbox = transform.transform_rect_bbox(rect);
    let x0 = bbox.x0.floor().max(0.0) as u32;
    let y0 = bbox.y0.floor().max(0.0) as u32;
    let x1 = bbox.x1.ceil().min(cw as f64) as u32;
    let y1 = bbox.y1.ceil().min(ch as f64) as u32;
    if x0 >= x1 || y0 >= y1 {
      return Ok(());
    }
    let mut img = image.borrow_mut();
    let mut rgba = img.to_rgba8();
    for yy in y0..y1 {
      for xx in x0..x1 {
        rgba.get_pixel_mut(xx, yy).0 = [0, 0, 0, 0];
      }
    }
    *img = DynamicImage::ImageRgba8(rgba);
    Ok(())
  }

  // -- text ---------------------------------------------------------------
  // Rendering is not yet implemented; these update no pixels but do not
  // throw, matching browser behaviour for the "API is wired up" case.
  // `measureText` returns coarse metrics derived from the font-size hint.

  fn fill_text(
    &self,
    #[webidl] text: String,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] max_width: Option<UnrestrictedDouble>,
  ) -> Result<(), JsErrorBox> {
    self.draw_text(&text, *x, *y, max_width.map(|m| *m), false)
  }

  fn stroke_text(
    &self,
    #[webidl] text: String,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] max_width: Option<UnrestrictedDouble>,
  ) -> Result<(), JsErrorBox> {
    self.draw_text(&text, *x, *y, max_width.map(|m| *m), true)
  }

  #[cppgc]
  fn measure_text(&self, #[webidl] text: String) -> TextMetrics {
    let (font_str, ls, ws, align, props) = {
      let inner = self.inner.borrow();
      let s = inner.cur();
      (
        s.font.clone(),
        parse_px(&s.letter_spacing),
        parse_px(&s.word_spacing),
        s.text_align,
        text_props(s),
      )
    };
    let parsed = text::parse_font(&font_str);
    let layout = text::layout(&text, &parsed, props, ls, ws);
    let m = text::metrics(&layout);
    let (width, left, right, ascent, descent) =
      (m.advance, -m.bbox_left, m.bbox_right, m.ascent, m.descent);
    let align_dx = match align {
      CanvasTextAlign::Start | CanvasTextAlign::Left => 0.0,
      CanvasTextAlign::End | CanvasTextAlign::Right => -width,
      CanvasTextAlign::Center => -width / 2.0,
    };
    TextMetrics {
      width,
      actual_bounding_box_left: left - align_dx,
      actual_bounding_box_right: right + align_dx,
      actual_bounding_box_ascent: ascent,
      actual_bounding_box_descent: descent,
      font_bounding_box_ascent: ascent,
      font_bounding_box_descent: descent,
      em_height_ascent: ascent,
      em_height_descent: descent,
      hanging_baseline: ascent * 0.8,
      alphabetic_baseline: 0.0,
      ideographic_baseline: -descent,
    }
  }

  // -- image data ---------------------------------------------------------

  #[cppgc]
  #[reentrant]
  fn create_image_data<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    arg0: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
  ) -> Result<ImageData, JsErrorBox> {
    if let Some(img) = unwrap_image_data(scope, arg0) {
      return Ok(ImageData {
        width: img.width,
        height: img.height,
        color_space: img.color_space,
        data: RefCell::new(vec![0; (img.width * img.height * 4) as usize]),
      });
    }
    let w = arg0
      .uint32_value(scope)
      .map(|v| v as u32)
      .unwrap_or(0)
      .max(1);
    let h = arg1
      .uint32_value(scope)
      .map(|v| v as u32)
      .unwrap_or(0)
      .max(1);
    let cs = parse_color_space_setting(scope, arg2);
    Ok(ImageData {
      width: w,
      height: h,
      color_space: cs,
      data: RefCell::new(vec![0; (w * h * 4) as usize]),
    })
  }

  #[cppgc]
  #[reentrant]
  fn get_image_data<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    #[webidl] sx: i32,
    #[webidl] sy: i32,
    #[webidl] sw: i32,
    #[webidl] sh: i32,
    arg4: v8::Local<'s, v8::Value>,
  ) -> Result<ImageData, JsErrorBox> {
    if sw == 0 || sh == 0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "ImageData width/height cannot be zero",
      ));
    }
    let (sx, sy, sw, sh) = normalise_rect(sx, sy, sw, sh);
    let cs = parse_color_space_setting(scope, arg4);
    let image = self.canvas_data()?;
    let img = image.borrow();
    let (cw, ch) = img.dimensions();
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for j in 0..sh {
      for i in 0..sw {
        let dx = sx as i64 + i as i64;
        let dy = sy as i64 + j as i64;
        if dx < 0 || dy < 0 || dx >= cw as i64 || dy >= ch as i64 {
          continue;
        }
        let p = img.get_pixel(dx as u32, dy as u32).0;
        let off = ((j * sw + i) * 4) as usize;
        out[off..off + 4].copy_from_slice(&p);
      }
    }
    Ok(ImageData {
      width: sw,
      height: sh,
      color_space: cs,
      data: RefCell::new(out),
    })
  }

  #[allow(clippy::too_many_arguments)]
  fn put_image_data<'s>(
    &self,
    #[webidl] image_data: Ref<ImageData>,
    #[webidl] dx: i32,
    #[webidl] dy: i32,
    arg3: v8::Local<'s, v8::Value>,
    arg4: v8::Local<'s, v8::Value>,
    arg5: v8::Local<'s, v8::Value>,
    arg6: v8::Local<'s, v8::Value>,
    scope: &mut v8::PinScope<'s, '_>,
  ) -> Result<(), JsErrorBox> {
    let iw = image_data.width as i32;
    let ih = image_data.height as i32;
    // Optional dirty rect.
    let (dirty_x, dirty_y, dirty_w, dirty_h) = if arg3.is_undefined() {
      (0, 0, iw, ih)
    } else {
      let n = |v: v8::Local<v8::Value>| v.int32_value(scope).unwrap_or(0);
      let mut x = n(arg3);
      let mut y = n(arg4);
      let mut w = n(arg5);
      let mut h = n(arg6);
      if w < 0 {
        x += w;
        w = -w;
      }
      if h < 0 {
        y += h;
        h = -h;
      }
      // Clamp to image bounds.
      let x0 = x.max(0).min(iw);
      let y0 = y.max(0).min(ih);
      let x1 = (x + w).max(0).min(iw);
      let y1 = (y + h).max(0).min(ih);
      (x0, y0, x1 - x0, y1 - y0)
    };
    if dirty_w <= 0 || dirty_h <= 0 {
      return Ok(());
    }
    let image = self.canvas_data()?;
    let (cw, ch) = image.borrow().dimensions();
    let mut img = image.borrow_mut();
    let mut rgba = img.to_rgba8();
    let src = image_data.data.borrow();
    for j in 0..dirty_h {
      for i in 0..dirty_w {
        let sx = dirty_x + i;
        let sy = dirty_y + j;
        let tx = dx + sx;
        let ty = dy + sy;
        if tx < 0 || ty < 0 || tx >= cw as i32 || ty >= ch as i32 {
          continue;
        }
        let off = ((sy * iw + sx) * 4) as usize;
        let pixel = [src[off], src[off + 1], src[off + 2], src[off + 3]];
        rgba.get_pixel_mut(tx as u32, ty as u32).0 = pixel;
      }
    }
    *img = DynamicImage::ImageRgba8(rgba);
    Ok(())
  }

  // -- drawImage -----------------------------------------------------------

  #[allow(clippy::too_many_arguments)]
  #[fast]
  fn draw_image<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    image: v8::Local<'s, v8::Value>,
    arg1: v8::Local<'s, v8::Value>,
    arg2: v8::Local<'s, v8::Value>,
    arg3: v8::Local<'s, v8::Value>,
    arg4: v8::Local<'s, v8::Value>,
    arg5: v8::Local<'s, v8::Value>,
    arg6: v8::Local<'s, v8::Value>,
    arg7: v8::Local<'s, v8::Value>,
    arg8: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let (rgba, sw_full, sh_full) = extract_image_source(scope, image)?;
    let n = |v: v8::Local<v8::Value>| v.number_value(scope).unwrap_or(f64::NAN);
    // Detect arity by counting non-undefined arguments.
    let count = [arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8]
      .iter()
      .filter(|v| !v.is_undefined())
      .count();
    let (sx, sy, sw, sh, dx, dy, dw, dh) = match count {
      2 => (
        0.0,
        0.0,
        sw_full as f64,
        sh_full as f64,
        n(arg1),
        n(arg2),
        sw_full as f64,
        sh_full as f64,
      ),
      4 => (
        0.0,
        0.0,
        sw_full as f64,
        sh_full as f64,
        n(arg1),
        n(arg2),
        n(arg3),
        n(arg4),
      ),
      8 => (
        n(arg1),
        n(arg2),
        n(arg3),
        n(arg4),
        n(arg5),
        n(arg6),
        n(arg7),
        n(arg8),
      ),
      _ => {
        return Err(JsErrorBox::type_error(
          "drawImage expects 3, 5 or 9 arguments",
        ));
      }
    };
    if !sx.is_finite()
      || !sy.is_finite()
      || !sw.is_finite()
      || !sh.is_finite()
      || !dx.is_finite()
      || !dy.is_finite()
      || !dw.is_finite()
      || !dh.is_finite()
    {
      return Ok(());
    }
    if sw == 0.0 || sh == 0.0 || dw == 0.0 || dh == 0.0 {
      return Ok(());
    }
    let blob = Blob::new(Arc::new(rgba));
    let img = Image::new(blob, ImageFormat::Rgba8, sw_full, sh_full);
    let (transform, _, _, alpha, blend, clip) = self.draw_state();
    // Composite source crop -> destination rect.
    let scale_x = dw / sw;
    let scale_y = dh / sh;
    let img_t = transform
      * Affine::translate((dx, dy))
      * Affine::scale_non_uniform(scale_x, scale_y)
      * Affine::translate((-sx, -sy));
    let smoothing = self.inner.borrow().cur().image_smoothing_enabled;
    let quality = if smoothing {
      self.inner.borrow().cur().image_smoothing_quality.into()
    } else {
      vello::peniko::ImageQuality::Low
    };
    let img = img.with_quality(quality).with_alpha(alpha);
    self.flush_with(blend, clip, |scene| {
      scene.draw_image(&img, img_t);
    })
  }

  // -- factories ----------------------------------------------------------

  #[cppgc]
  fn create_linear_gradient(
    &self,
    #[webidl] x0: UnrestrictedDouble,
    #[webidl] y0: UnrestrictedDouble,
    #[webidl] x1: UnrestrictedDouble,
    #[webidl] y1: UnrestrictedDouble,
  ) -> CanvasGradient {
    CanvasGradient::new_linear(*x0, *y0, *x1, *y1)
  }

  #[cppgc]
  #[allow(clippy::too_many_arguments)]
  fn create_radial_gradient(
    &self,
    #[webidl] x0: UnrestrictedDouble,
    #[webidl] y0: UnrestrictedDouble,
    #[webidl] r0: UnrestrictedDouble,
    #[webidl] x1: UnrestrictedDouble,
    #[webidl] y1: UnrestrictedDouble,
    #[webidl] r1: UnrestrictedDouble,
  ) -> Result<CanvasGradient, JsErrorBox> {
    CanvasGradient::new_radial(*x0, *y0, *r0, *x1, *y1, *r1)
  }

  #[cppgc]
  fn create_conic_gradient(
    &self,
    #[webidl] start_angle: UnrestrictedDouble,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) -> CanvasGradient {
    CanvasGradient::new_conic(*start_angle, *x, *y)
  }

  fn create_pattern<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    image: v8::Local<'s, v8::Value>,
    #[webidl] repetition: String,
  ) -> Result<Option<v8::Global<v8::Value>>, JsErrorBox> {
    let rep = PatternRepetition::from_string(&repetition).ok_or_else(|| {
      JsErrorBox::type_error(format!("Invalid repetition: {repetition}"))
    })?;
    let (rgba, w, h) = extract_image_source(scope, image)?;
    let pattern = CanvasPattern {
      width: w,
      height: h,
      rgba,
      repetition: rep,
      transform: RefCell::new(Affine::IDENTITY),
    };
    let obj = deno_core::cppgc::make_cppgc_object(scope, pattern);
    Ok(Some(v8::Global::new(scope, obj.cast())))
  }
}

impl CanvasRenderingContext2D {
  fn draw_text(
    &self,
    text_str: &str,
    x: f64,
    y: f64,
    max_width: Option<f64>,
    stroke: bool,
  ) -> Result<(), JsErrorBox> {
    let (
      font_str,
      letter_spacing,
      word_spacing,
      align,
      baseline,
      transform,
      brush,
      alpha,
      blend,
      clip,
      stroke_def,
      props,
    ) = {
      let inner = self.inner.borrow();
      let s = inner.cur();
      let (mix, compose) = s.global_composite_operation.to_blend();
      let blend = vello::peniko::BlendMode::new(mix, compose);
      let brush = if stroke {
        s.stroke_style.brush()
      } else {
        s.fill_style.brush()
      };
      (
        s.font.clone(),
        parse_px(&s.letter_spacing),
        parse_px(&s.word_spacing),
        s.text_align,
        s.text_baseline,
        s.transform,
        brush,
        s.global_alpha,
        blend,
        s.clip.clone(),
        s.stroke(),
        text_props(s),
      )
    };
    let parsed = text::parse_font(&font_str);
    let layout =
      text::layout(text_str, &parsed, props, letter_spacing, word_spacing);
    let m = text::metrics(&layout);
    if m.advance == 0.0 {
      return Ok(());
    }
    let scale_x = match max_width {
      Some(mw) if mw > 0.0 && m.advance > mw => mw / m.advance,
      _ => 1.0,
    };
    let (dx, dy) = text::alignment_origin(&m, align, baseline);
    let origin_x = x + dx;
    let origin_y = y + dy;
    self.flush_with(blend, clip, |scene| {
      let final_brush = alpha_brush(&brush, alpha);
      text::for_each_run(&layout, scale_x, |font, font_size, glyphs| {
        let run_t =
          transform * vello::kurbo::Affine::translate((origin_x, origin_y));
        let style: vello::peniko::StyleRef = if stroke {
          (&stroke_def).into()
        } else {
          vello::peniko::Fill::NonZero.into()
        };
        scene
          .draw_glyphs(font)
          .font_size(font_size)
          .transform(run_t)
          .brush(&final_brush)
          .brush_alpha(1.0)
          .draw(style, glyphs);
      });
    })
  }

  fn canvas_data(&self) -> Result<Rc<RefCell<DynamicImage>>, JsErrorBox> {
    Ok(self.bitmap.clone())
  }

  /// Resize the backing bitmap for a surface-backed context to match new
  /// surface dimensions. Pixels in the overlap are preserved, the rest is
  /// transparent black.
  pub(crate) fn resize_surface(
    &self,
    width: u32,
    height: u32,
  ) -> Result<(), JsErrorBox> {
    let ContextData::Surface(_) = &self.data else {
      return Ok(());
    };
    {
      let mut img = self.bitmap.borrow_mut();
      let new_img = DynamicImage::new(width, height, image::ColorType::Rgba8);
      *img = new_img;
    }
    if let Some(s) = self.surface_only.as_ref() {
      let ContextData::Surface(surface_data) = &self.data else {
        unreachable!()
      };
      let id = surface_data.borrow().id;
      s.resize_to(id, width, height)?;
    }
    Ok(())
  }

  /// Push the current bitmap to the surface and submit. Caller is responsible
  /// for invoking `surface_present` on the wgpu instance afterwards.
  pub(crate) fn present_surface(&self) -> Result<(), JsErrorBox> {
    let ContextData::Surface(surface_data) = &self.data else {
      return Err(JsErrorBox::new(
        "DOMExceptionInvalidStateError",
        "Context2D is not surface-backed",
      ));
    };
    let surface_only = self.surface_only.as_ref().ok_or_else(|| {
      JsErrorBox::new(
        "DOMExceptionInvalidStateError",
        "Context2D surface pipeline missing",
      )
    })?;
    let surface_id = surface_data.borrow().id;
    let img = self.bitmap.borrow();
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();
    surface_only.upload_and_render(surface_id, &rgba, w, h)
  }

  fn clear_bitmap(&self) -> Result<(), JsErrorBox> {
    let image = self.canvas_data()?;
    let mut img = image.borrow_mut();
    let (w, h) = img.dimensions();
    *img = DynamicImage::new(w, h, image::ColorType::Rgba8);
    Ok(())
  }

  fn draw_state(
    &self,
  ) -> (
    Affine,
    Brush,
    Option<Affine>,
    f32,
    BlendMode,
    Option<(BezPath, CanvasFillRule, Affine)>,
  ) {
    let inner = self.inner.borrow();
    let s = inner.cur();
    let (mix, compose) = s.global_composite_operation.to_blend();
    let blend = BlendMode::new(mix, compose);
    let brush = s.fill_style.brush();
    let brush_t = match &s.fill_style {
      StyleBrush::Object { .. } => style_brush_transform(&s.fill_style),
      _ => None,
    };
    (
      s.transform,
      brush,
      brush_t,
      s.global_alpha,
      blend,
      s.clip.clone(),
    )
  }

  fn stroke_brush(&self) -> (Brush, Option<Affine>) {
    let inner = self.inner.borrow();
    let s = inner.cur();
    let brush = s.stroke_style.brush();
    let bt = match &s.stroke_style {
      StyleBrush::Object { .. } => style_brush_transform(&s.stroke_style),
      _ => None,
    };
    (brush, bt)
  }

  /// Build a fresh `Scene` via `f`, render it on top of the current bitmap,
  /// and store the composited result back into the canvas's `DynamicImage`.
  fn flush_with<F: FnOnce(&mut Scene)>(
    &self,
    blend: BlendMode,
    clip: Option<(BezPath, CanvasFillRule, Affine)>,
    f: F,
  ) -> Result<(), JsErrorBox> {
    let image = self.canvas_data()?;
    let (width, height) = image.borrow().dimensions();
    if width == 0 || height == 0 {
      return Ok(());
    }
    let mut scene = Scene::new();
    let layered = blend.mix != vello::peniko::Mix::Normal
      || blend.compose != vello::peniko::Compose::SrcOver
      || clip.is_some();
    if layered {
      // Push a layer for clip and blend mode together (they're applied at pop).
      if let Some((path, _, t)) = &clip {
        scene.push_layer(blend, 1.0, *t, path);
      } else {
        scene.push_layer(
          blend,
          1.0,
          Affine::IDENTITY,
          &Rect::new(0.0, 0.0, width as f64, height as f64),
        );
      }
    }
    f(&mut scene);
    if layered {
      scene.pop_layer();
    }
    let mut overlay = renderer::render_scene_to_rgba(
      &scene,
      width,
      height,
      Color::TRANSPARENT,
    )?;

    // Filter chain runs first (CSS filters apply to the source).
    let (filters, shadow) = {
      let inner = self.inner.borrow();
      let s = inner.cur();
      (
        effects::parse_filter(&s.filter),
        if shadow_active(s) {
          Some((
            s.shadow_color,
            s.shadow_offset_x,
            s.shadow_offset_y,
            s.shadow_blur,
          ))
        } else {
          None
        },
      )
    };
    if !filters.is_empty() {
      effects::apply_filters(&mut overlay, width, height, &filters);
    }

    let mut img = image.borrow_mut();
    let mut base = img.to_rgba8();

    // Shadow is drawn underneath the source (per spec).
    if let Some((color, dx, dy, blur)) = shadow {
      let shadow_buf =
        effects::make_shadow(&overlay, width, height, color, blur);
      effects::composite_overlay_at(&mut base, &shadow_buf, dx, dy);
    }

    composite_over(&mut base, &overlay);
    *img = DynamicImage::ImageRgba8(base);
    Ok(())
  }
}

/// Parse a CSS length string as pixels, treating `em`/`rem` as 16px and
/// returning 0 for unparseable / "normal" / empty.
fn parse_px(s: &str) -> f32 {
  let s = s.trim();
  if s.is_empty() || s == "normal" {
    return 0.0;
  }
  let bytes = s.as_bytes();
  let mut i = 0;
  while i < bytes.len()
    && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'-')
  {
    i += 1;
  }
  if i == 0 {
    return 0.0;
  }
  let n: f32 = s[..i].parse().unwrap_or(0.0);
  let unit = &s[i..];
  let scale = match unit {
    "" | "px" => 1.0,
    "pt" => 96.0 / 72.0,
    "em" | "rem" => 16.0,
    _ => 1.0,
  };
  n * scale
}

fn text_props(s: &DrawState) -> text::TextProps {
  // `fontStretch` defaults to Normal. We only override the parsed-from-font
  // width when the user has explicitly set it to something else.
  let stretch_override = match s.font_stretch {
    state::CanvasFontStretch::Normal => None,
    other => Some(text::stretch_to_width(other)),
  };
  text::TextProps {
    stretch_override,
    kerning: s.font_kerning,
    variant_caps: s.font_variant_caps,
    rendering: s.text_rendering,
  }
}

fn shadow_active(s: &DrawState) -> bool {
  let alpha = s.shadow_color.to_rgba8().to_u8_array()[3];
  if alpha == 0 {
    return false;
  }
  s.shadow_blur > 0.0 || s.shadow_offset_x != 0.0 || s.shadow_offset_y != 0.0
}

// ---- helpers --------------------------------------------------------------

fn alpha_brush(brush: &Brush, alpha: f32) -> Brush {
  if (alpha - 1.0).abs() < f32::EPSILON {
    return brush.clone();
  }
  brush.clone().multiply_alpha(alpha)
}

fn composite_over(base: &mut RgbaImage, overlay: &[u8]) {
  let buf = base.as_flat_samples_mut().samples;
  debug_assert_eq!(buf.len(), overlay.len());
  for i in (0..buf.len()).step_by(4) {
    let sa = overlay[i + 3] as u32;
    if sa == 0 {
      continue;
    }
    if sa == 255 {
      buf[i] = overlay[i];
      buf[i + 1] = overlay[i + 1];
      buf[i + 2] = overlay[i + 2];
      buf[i + 3] = 255;
      continue;
    }
    let inv = 255 - sa;
    for c in 0..3 {
      let s = overlay[i + c] as u32;
      let d = buf[i + c] as u32;
      buf[i + c] = ((s * 255 + d * inv) / 255) as u8;
    }
    let da = buf[i + 3] as u32;
    buf[i + 3] = (sa + (da * inv) / 255) as u8;
  }
}

fn parse_style<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Option<StyleBrush> {
  if value.is_string() {
    let s = value.to_rust_string_lossy(scope);
    return parse_color(&s).map(StyleBrush::Solid);
  }
  if let Some(g) = deno_core::cppgc::try_unwrap_cppgc_persistent_object::<
    CanvasGradient,
  >(scope, value)
  {
    let brush = g.to_brush();
    let obj: v8::Local<v8::Object> = value.try_into().ok()?;
    return Some(StyleBrush::Object {
      brush,
      object: v8::Global::new(scope, obj),
    });
  }
  if let Some(p) = deno_core::cppgc::try_unwrap_cppgc_persistent_object::<
    CanvasPattern,
  >(scope, value)
  {
    let brush = p.to_brush();
    let obj: v8::Local<v8::Object> = value.try_into().ok()?;
    return Some(StyleBrush::Object {
      brush,
      object: v8::Global::new(scope, obj),
    });
  }
  None
}

fn style_to_v8<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  brush: &StyleBrush,
) -> v8::Local<'s, v8::Value> {
  match brush {
    StyleBrush::Solid(c) => {
      v8::String::new(scope, &color_to_string(*c)).unwrap().into()
    }
    StyleBrush::Object { object, .. } => v8::Local::new(scope, object).into(),
  }
}

fn style_brush_transform(brush: &StyleBrush) -> Option<Affine> {
  // For now, only patterns carry a brush-side transform (gradients bake their
  // geometry into the encoded points). Patterns store it on the JS object;
  // since we don't reach back into JS for a setTransform readback here, we
  // return None and rely on the bake-time transform applied via setTransform.
  let _ = brush;
  None
}

fn resolve_path_args<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  arg0: v8::Local<'s, v8::Value>,
  arg1: v8::Local<'s, v8::Value>,
) -> Result<(Option<BezPath>, CanvasFillRule), JsErrorBox> {
  if let Some(p) = unwrap_path2d(scope, arg0) {
    let rule = parse_fill_rule(scope, arg1)?;
    return Ok((Some(p.builder.borrow().bez.clone()), rule));
  }
  let rule = parse_fill_rule(scope, arg0)?;
  Ok((None, rule))
}

fn parse_fill_rule<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Result<CanvasFillRule, JsErrorBox> {
  if value.is_undefined() || value.is_null() {
    return Ok(CanvasFillRule::Nonzero);
  }
  let s = value.to_rust_string_lossy(scope);
  match s.as_str() {
    "nonzero" => Ok(CanvasFillRule::Nonzero),
    "evenodd" => Ok(CanvasFillRule::Evenodd),
    _ => Err(JsErrorBox::type_error(format!(
      "Invalid CanvasFillRule: {s}"
    ))),
  }
}

fn unwrap_path2d<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Option<deno_core::cppgc::Ref<Path2D>> {
  deno_core::cppgc::try_unwrap_cppgc_persistent_object::<Path2D>(scope, value)
}

fn unwrap_image_data<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Option<deno_core::cppgc::Ref<ImageData>> {
  deno_core::cppgc::try_unwrap_cppgc_persistent_object::<ImageData>(
    scope, value,
  )
}

fn extract_image_source<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Result<(Vec<u8>, u32, u32), JsErrorBox> {
  if let Some(b) = deno_core::cppgc::try_unwrap_cppgc_persistent_object::<
    ImageBitmap,
  >(scope, value)
  {
    if b.detached.get().is_some() {
      return Err(JsErrorBox::new(
        "DOMExceptionInvalidStateError",
        "ImageBitmap is detached",
      ));
    }
    let img = b.data.borrow();
    let (w, h) = img.dimensions();
    return Ok((img.to_rgba8().into_raw(), w, h));
  }
  if let Some(canvas) = deno_core::cppgc::try_unwrap_cppgc_persistent_object::<
    crate::canvas::OffscreenCanvas,
  >(scope, value)
  {
    let img = canvas.data.borrow();
    let (w, h) = img.dimensions();
    return Ok((img.to_rgba8().into_raw(), w, h));
  }
  if let Some(img) = deno_core::cppgc::try_unwrap_cppgc_persistent_object::<
    ImageData,
  >(scope, value)
  {
    return Ok((img.data.borrow().clone(), img.width, img.height));
  }
  Err(JsErrorBox::type_error(
    "Expected an ImageBitmap, OffscreenCanvas or ImageData",
  ))
}

fn parse_color_space_setting<'s>(
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
  match v.to_rust_string_lossy(scope).as_str() {
    "display-p3" => PredefinedColorSpace::DisplayP3,
    _ => PredefinedColorSpace::Srgb,
  }
}

fn normalise_rect(x: i32, y: i32, w: i32, h: i32) -> (i32, i32, u32, u32) {
  let mut x = x;
  let mut y = y;
  let mut w = w;
  let mut h = h;
  if w < 0 {
    x += w;
    w = -w;
  }
  if h < 0 {
    y += h;
    h = -h;
  }
  (x, y, w as u32, h as u32)
}

#[derive(WebIDL)]
#[webidl(dictionary)]
struct CanvasRenderingContext2DSettings {
  #[webidl(default = true)]
  alpha: bool,
}

pub fn create<'s>(
  instance: Option<deno_webgpu::Instance>,
  canvas: v8::Global<v8::Object>,
  data: ContextData,
  scope: &mut v8::PinScope<'s, '_>,
  options: v8::Local<'s, v8::Value>,
  prefix: &'static str,
  context: &'static str,
) -> Result<v8::Global<v8::Value>, JsErrorBox> {
  use deno_core::webidl::WebIdlConverter;
  let settings = CanvasRenderingContext2DSettings::convert(
    scope,
    options,
    prefix.into(),
    (|| context.into()).into(),
    &(),
  )
  .map_err(JsErrorBox::from_err)?;

  let (bitmap, surface_only) = match &data {
    ContextData::Canvas(image) => (image.clone(), None),
    ContextData::Surface(surface_data) => {
      let deno_webgpu::canvas::SurfaceData {
        id, width, height, ..
      } = &*surface_data.borrow();
      let pipeline = crate::bitmaprenderer::SurfaceBitmap::new_for_surface(
        instance.unwrap(),
        *id,
        *width,
        *height,
      )?;
      let img = Rc::new(RefCell::new(DynamicImage::new(
        *width,
        *height,
        image::ColorType::Rgba8,
      )));
      (img, Some(pipeline))
    }
  };

  let ctx = CanvasRenderingContext2D {
    canvas,
    bitmap,
    data,
    surface_only,
    alpha: settings.alpha,
    inner: RefCell::new(Inner {
      state_stack: vec![DrawState::default()],
      path: PathBuilder::default(),
    }),
  };

  let obj = deno_core::cppgc::make_cppgc_object(scope, ctx);
  Ok(v8::Global::new(scope, obj.cast()))
}

use std::sync::Arc;
