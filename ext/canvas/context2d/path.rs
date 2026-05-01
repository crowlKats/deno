// Copyright 2018-2026 the Deno authors. MIT license.
//
// Path-building helpers shared between `CanvasRenderingContext2D` (current
// path) and the `Path2D` interface.

use deno_core::GarbageCollected;
use deno_core::cppgc::Ref;
use deno_core::op2;
use deno_core::v8;
use deno_core::v8::cppgc::Visitor;
use deno_core::webidl::UnrestrictedDouble;
use deno_core::webidl::WebIdlInterfaceConverter;
use deno_error::JsErrorBox;
use vello::kurbo::Affine;
use vello::kurbo::Arc;
use vello::kurbo::BezPath;
use vello::kurbo::Point;
use vello::kurbo::Rect;
use vello::kurbo::RoundedRect;
use vello::kurbo::RoundedRectRadii;
use vello::kurbo::Shape;
use vello::kurbo::Vec2;

use super::state::CanvasFillRule;

/// Mutable path-building state. Wrapped in a `RefCell` by both
/// `CanvasRenderingContext2D` and `Path2D`.
#[derive(Default, Clone)]
pub struct PathBuilder {
  pub bez: BezPath,
  /// Last sub-path start, used by `closePath`.
  pub start: Option<Point>,
  /// Pen position for path building.
  pub pen: Point,
}

impl PathBuilder {
  pub fn reset(&mut self) {
    self.bez = BezPath::new();
    self.start = None;
    self.pen = Point::ZERO;
  }

  pub fn ensure_subpath(&mut self) {
    if self.start.is_none() {
      let p = self.pen;
      self.bez.move_to(p);
      self.start = Some(p);
    }
  }

  pub fn close_path(&mut self) {
    if let Some(start) = self.start {
      self.bez.close_path();
      self.pen = start;
    }
  }

  pub fn move_to(&mut self, x: f64, y: f64) {
    let p = Point::new(x, y);
    self.bez.move_to(p);
    self.pen = p;
    self.start = Some(p);
  }

  pub fn line_to(&mut self, x: f64, y: f64) {
    self.ensure_subpath();
    let p = Point::new(x, y);
    self.bez.line_to(p);
    self.pen = p;
  }

  pub fn quad_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
    self.ensure_subpath();
    let p = Point::new(x, y);
    self.bez.quad_to(Point::new(cpx, cpy), p);
    self.pen = p;
  }

  pub fn bezier_to(
    &mut self,
    cp1x: f64,
    cp1y: f64,
    cp2x: f64,
    cp2y: f64,
    x: f64,
    y: f64,
  ) {
    self.ensure_subpath();
    let p = Point::new(x, y);
    self
      .bez
      .curve_to(Point::new(cp1x, cp1y), Point::new(cp2x, cp2y), p);
    self.pen = p;
  }

  pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
    let r = Rect::new(x, y, x + w, y + h);
    self.bez.move_to((r.x0, r.y0));
    self.bez.line_to((r.x1, r.y0));
    self.bez.line_to((r.x1, r.y1));
    self.bez.line_to((r.x0, r.y1));
    self.bez.close_path();
    self.pen = Point::new(x, y);
    self.start = Some(self.pen);
  }

  pub fn round_rect(
    &mut self,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    radii: RoundedRectRadii,
  ) {
    let rr = RoundedRect::from_rect(Rect::new(x, y, x + w, y + h), radii);
    let mut first = true;
    for el in rr.path_elements(0.1) {
      match el {
        vello::kurbo::PathEl::MoveTo(p) => {
          if first {
            self.bez.move_to(p);
            self.start = Some(p);
            first = false;
          } else {
            self.bez.move_to(p);
          }
          self.pen = p;
        }
        vello::kurbo::PathEl::LineTo(p) => {
          self.bez.line_to(p);
          self.pen = p;
        }
        vello::kurbo::PathEl::QuadTo(c, p) => {
          self.bez.quad_to(c, p);
          self.pen = p;
        }
        vello::kurbo::PathEl::CurveTo(c1, c2, p) => {
          self.bez.curve_to(c1, c2, p);
          self.pen = p;
        }
        vello::kurbo::PathEl::ClosePath => {
          self.bez.close_path();
        }
      }
    }
  }

  pub fn arc(
    &mut self,
    x: f64,
    y: f64,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    counterclockwise: bool,
  ) -> Result<(), JsErrorBox> {
    if radius < 0.0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "Negative radius",
      ));
    }
    self.ellipse(
      x,
      y,
      radius,
      radius,
      0.0,
      start_angle,
      end_angle,
      counterclockwise,
    )
  }

  pub fn ellipse(
    &mut self,
    x: f64,
    y: f64,
    radius_x: f64,
    radius_y: f64,
    rotation: f64,
    start_angle: f64,
    end_angle: f64,
    counterclockwise: bool,
  ) -> Result<(), JsErrorBox> {
    if radius_x < 0.0 || radius_y < 0.0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "Negative radius",
      ));
    }
    // Per spec, normalise sweep angle to (-2π..=2π) then rebuild end.
    let two_pi = std::f64::consts::TAU;
    let mut sweep = end_angle - start_angle;
    if !counterclockwise {
      if sweep < 0.0 {
        sweep = sweep.rem_euclid(two_pi);
      } else if sweep > two_pi {
        sweep = two_pi;
      }
    } else if sweep > 0.0 {
      sweep = -((-sweep).rem_euclid(two_pi));
    } else if sweep < -two_pi {
      sweep = -two_pi;
    }
    let arc = Arc::new(
      (x, y),
      Vec2::new(radius_x, radius_y),
      start_angle,
      sweep,
      rotation,
    );
    let arc_start = sample_ellipse_point(
      Point::new(x, y),
      Vec2::new(radius_x, radius_y),
      rotation,
      start_angle,
    );
    if self.start.is_none() {
      self.bez.move_to(arc_start);
      self.start = Some(arc_start);
    } else {
      self.bez.line_to(arc_start);
    }
    self.bez.extend(arc.append_iter(0.1));
    self.pen = sample_ellipse_point(
      Point::new(x, y),
      Vec2::new(radius_x, radius_y),
      rotation,
      start_angle + sweep,
    );
    Ok(())
  }

  pub fn arc_to(
    &mut self,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    radius: f64,
  ) -> Result<(), JsErrorBox> {
    if radius < 0.0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "Negative radius",
      ));
    }
    self.ensure_subpath();
    let p0 = self.pen;
    let p1 = Point::new(x1, y1);
    let p2 = Point::new(x2, y2);
    if p0 == p1 || p1 == p2 || radius == 0.0 {
      self.bez.line_to(p1);
      self.pen = p1;
      return Ok(());
    }
    // Compute the angle bisector and tangent points.
    let v0 = (p0 - p1).normalize();
    let v2 = (p2 - p1).normalize();
    let cos_theta = v0.dot(v2).clamp(-1.0, 1.0);
    let sin_half = ((1.0 - cos_theta) * 0.5).sqrt();
    if sin_half.abs() < 1e-10 {
      // Collinear; fall back to a line.
      self.bez.line_to(p1);
      self.pen = p1;
      return Ok(());
    }
    let tan_half = sin_half / ((1.0 + cos_theta) * 0.5).sqrt();
    let dist = radius / tan_half;
    let t0 = p1 + v0 * dist;
    let t2 = p1 + v2 * dist;
    let bisector = (v0 + v2).normalize();
    let center_dist = radius / sin_half;
    let center = p1 + bisector * center_dist;

    let start_angle = (t0 - center).atan2();
    let end_angle = (t2 - center).atan2();
    let cross = v0.cross(v2);
    let mut sweep = end_angle - start_angle;
    let two_pi = std::f64::consts::TAU;
    if cross < 0.0 {
      // CCW arc
      if sweep > 0.0 {
        sweep -= two_pi;
      }
    } else if sweep < 0.0 {
      sweep += two_pi;
    }
    let arc =
      Arc::new(center, Vec2::new(radius, radius), start_angle, sweep, 0.0);
    self.bez.line_to(t0);
    self.bez.extend(arc.append_iter(0.1));
    self.pen = t2;
    Ok(())
  }

  pub fn add_path(&mut self, other: &BezPath, transform: Affine) {
    for el in other.elements() {
      let el = match *el {
        vello::kurbo::PathEl::MoveTo(p) => {
          let p = transform * p;
          self.start = Some(p);
          self.pen = p;
          vello::kurbo::PathEl::MoveTo(p)
        }
        vello::kurbo::PathEl::LineTo(p) => {
          let p = transform * p;
          self.pen = p;
          vello::kurbo::PathEl::LineTo(p)
        }
        vello::kurbo::PathEl::QuadTo(c, p) => {
          let p = transform * p;
          self.pen = p;
          vello::kurbo::PathEl::QuadTo(transform * c, p)
        }
        vello::kurbo::PathEl::CurveTo(c1, c2, p) => {
          let p = transform * p;
          self.pen = p;
          vello::kurbo::PathEl::CurveTo(transform * c1, transform * c2, p)
        }
        vello::kurbo::PathEl::ClosePath => {
          if let Some(s) = self.start {
            self.pen = s;
          }
          vello::kurbo::PathEl::ClosePath
        }
      };
      self.bez.push(el);
    }
  }
}

fn sample_ellipse_point(
  center: Point,
  radii: Vec2,
  rotation: f64,
  angle: f64,
) -> Point {
  let (sa, ca) = angle.sin_cos();
  let (sr, cr) = rotation.sin_cos();
  let x = radii.x * ca;
  let y = radii.y * sa;
  Point::new(center.x + x * cr - y * sr, center.y + x * sr + y * cr)
}

/// Hit-test a point against a filled path.
pub fn point_in_path(path: &BezPath, pt: Point, rule: CanvasFillRule) -> bool {
  let w = path.winding(pt);
  match rule {
    CanvasFillRule::Nonzero => w != 0,
    CanvasFillRule::Evenodd => w & 1 != 0,
  }
}

// ---------------------------------------------------------------------------
// Path2D
// ---------------------------------------------------------------------------

pub struct Path2D {
  pub builder: std::cell::RefCell<PathBuilder>,
}

// SAFETY: holds no v8 references.
unsafe impl GarbageCollected for Path2D {
  fn trace(&self, _visitor: &mut Visitor) {}
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"Path2D"
  }
}

impl WebIdlInterfaceConverter for Path2D {
  const NAME: &'static str = "Path2D";
}

#[op2]
impl Path2D {
  /// Construct from another `Path2D`, an SVG path string, or empty.
  #[constructor]
  #[cppgc]
  fn new<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    arg: v8::Local<'s, v8::Value>,
  ) -> Result<Path2D, JsErrorBox> {
    let bez = if arg.is_undefined() || arg.is_null() {
      BezPath::new()
    } else if arg.is_string() {
      let s = arg.to_rust_string_lossy(scope);
      BezPath::from_svg(&s)
        .map_err(|e| JsErrorBox::type_error(format!("Invalid SVG path: {e}")))?
    } else if let Some(other) =
      deno_core::cppgc::try_unwrap_cppgc_persistent_object::<Path2D>(scope, arg)
    {
      other.builder.borrow().bez.clone()
    } else {
      return Err(JsErrorBox::type_error(
        "Path2D constructor expects an optional Path2D or DOMString",
      ));
    };
    let pen = bez
      .elements()
      .last()
      .and_then(|e| match e {
        vello::kurbo::PathEl::MoveTo(p)
        | vello::kurbo::PathEl::LineTo(p)
        | vello::kurbo::PathEl::QuadTo(_, p)
        | vello::kurbo::PathEl::CurveTo(_, _, p) => Some(*p),
        vello::kurbo::PathEl::ClosePath => None,
      })
      .unwrap_or(Point::ZERO);
    Ok(Path2D {
      builder: std::cell::RefCell::new(PathBuilder {
        bez,
        start: None,
        pen,
      }),
    })
  }

  #[reentrant]
  fn add_path<'s>(
    &self,
    scope: &mut v8::PinScope<'s, '_>,
    #[webidl] path: Ref<Path2D>,
    transform: v8::Local<'s, v8::Value>,
  ) -> Result<(), JsErrorBox> {
    let t = parse_dom_matrix_init(scope, transform)?;
    let other_bez = path.builder.borrow().bez.clone();
    self.builder.borrow_mut().add_path(&other_bez, t);
    Ok(())
  }

  #[fast]
  fn close_path(&self) {
    self.builder.borrow_mut().close_path();
  }

  fn move_to(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.builder.borrow_mut().move_to(*x, *y);
  }

  fn line_to(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.builder.borrow_mut().line_to(*x, *y);
  }

  fn quadratic_curve_to(
    &self,
    #[webidl] cpx: UnrestrictedDouble,
    #[webidl] cpy: UnrestrictedDouble,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
  ) {
    self.builder.borrow_mut().quad_to(*cpx, *cpy, *x, *y);
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
      .builder
      .borrow_mut()
      .bezier_to(*cp1x, *cp1y, *cp2x, *cp2y, *x, *y);
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
      .builder
      .borrow_mut()
      .arc_to(*x1, *y1, *x2, *y2, *radius)
  }

  fn rect(
    &self,
    #[webidl] x: UnrestrictedDouble,
    #[webidl] y: UnrestrictedDouble,
    #[webidl] w: UnrestrictedDouble,
    #[webidl] h: UnrestrictedDouble,
  ) {
    self.builder.borrow_mut().rect(*x, *y, *w, *h);
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
    self.builder.borrow_mut().round_rect(*x, *y, *w, *h, rr);
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
    self.builder.borrow_mut().arc(
      *x,
      *y,
      *radius,
      *start_angle,
      *end_angle,
      counterclockwise.unwrap_or(false),
    )
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
    self.builder.borrow_mut().ellipse(
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
}

/// Parse a DOMMatrix2DInit / DOMMatrixInit / DOMMatrix value into a kurbo
/// affine, following the WebIDL "fixup" rules: the 3D `m11/m12/m21/m22/m41/m42`
/// fields take priority over the 2D `a/b/c/d/e/f` aliases, and supplying
/// inconsistent values throws a TypeError. `null` / `undefined` is identity.
pub fn parse_dom_matrix_init<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Result<Affine, JsErrorBox> {
  if value.is_undefined() || value.is_null() {
    return Ok(Affine::IDENTITY);
  }
  let obj: v8::Local<v8::Object> = value
    .try_into()
    .map_err(|_| JsErrorBox::type_error("Expected DOMMatrix2DInit object"))?;
  let read = |scope: &mut v8::PinScope<'s, '_>, key: &str| -> Option<f64> {
    let k = v8::String::new(scope, key).unwrap();
    let v = obj.get(scope, k.into())?;
    if v.is_undefined() {
      return None;
    }
    v.number_value(scope)
  };
  let fixup = |scope: &mut v8::PinScope<'s, '_>,
               three_d: &str,
               two_d: &str,
               default: f64|
   -> Result<f64, JsErrorBox> {
    let a = read(scope, three_d);
    let b = read(scope, two_d);
    match (a, b) {
      (Some(a), Some(b)) => {
        if a == b || (a.is_nan() && b.is_nan()) {
          Ok(a)
        } else {
          Err(JsErrorBox::type_error(format!(
            "Inconsistent matrix init: {three_d}={a} but {two_d}={b}"
          )))
        }
      }
      (Some(a), None) => Ok(a),
      (None, Some(b)) => Ok(b),
      (None, None) => Ok(default),
    }
  };
  let a = fixup(scope, "m11", "a", 1.0)?;
  let b = fixup(scope, "m12", "b", 0.0)?;
  let c = fixup(scope, "m21", "c", 0.0)?;
  let d = fixup(scope, "m22", "d", 1.0)?;
  let e = fixup(scope, "m41", "e", 0.0)?;
  let f = fixup(scope, "m42", "f", 0.0)?;
  Ok(Affine::new([a, b, c, d, e, f]))
}

/// Parse the `radii` argument of `roundRect`. Accepts a number, a DOMPoint-init,
/// or an array of 1–4 of those. For now numbers are supported (the common case);
/// DOMPoint-init values fall back to using the `x` field.
pub fn parse_round_rect_radii<'s>(
  scope: &mut v8::PinScope<'s, '_>,
  value: v8::Local<'s, v8::Value>,
) -> Result<RoundedRectRadii, JsErrorBox> {
  if value.is_undefined() {
    return Ok(RoundedRectRadii::from_single_radius(0.0));
  }
  if value.is_number() {
    let n = value.number_value(scope).unwrap_or(0.0);
    if n < 0.0 {
      return Err(JsErrorBox::new("DOMExceptionRangeError", "Negative radius"));
    }
    return Ok(RoundedRectRadii::from_single_radius(n));
  }
  if value.is_array() {
    let arr: v8::Local<v8::Array> = value.try_into().unwrap();
    let len = arr.length() as usize;
    let read = |i: u32| -> f64 {
      arr
        .get_index(scope, i)
        .and_then(|v| v.number_value(scope))
        .unwrap_or(0.0)
    };
    return match len {
      1 => {
        let r = read(0);
        Ok(RoundedRectRadii::from_single_radius(r))
      }
      2 => {
        let a = read(0);
        let b = read(1);
        Ok(RoundedRectRadii::new(a, b, a, b))
      }
      3 => {
        let a = read(0);
        let b = read(1);
        let c = read(2);
        Ok(RoundedRectRadii::new(a, b, c, b))
      }
      4 => Ok(RoundedRectRadii::new(read(0), read(1), read(2), read(3))),
      _ => Err(JsErrorBox::new(
        "DOMExceptionRangeError",
        "roundRect radii must have 1 to 4 elements",
      )),
    };
  }
  // Could be a DOMPoint-init object — we read its `x` field as the radius.
  if let Ok(obj) = TryInto::<v8::Local<v8::Object>>::try_into(value) {
    let k = v8::String::new(scope, "x").unwrap();
    if let Some(v) = obj.get(scope, k.into()) {
      if let Some(n) = v.number_value(scope) {
        return Ok(RoundedRectRadii::from_single_radius(n));
      }
    }
  }
  Ok(RoundedRectRadii::from_single_radius(0.0))
}
