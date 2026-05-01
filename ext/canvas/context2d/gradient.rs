// Copyright 2018-2026 the Deno authors. MIT license.

use std::cell::RefCell;

use deno_core::GarbageCollected;
use deno_core::op2;
use deno_core::v8::cppgc::Visitor;
use deno_core::webidl::UnrestrictedDouble;
use deno_core::webidl::WebIdlInterfaceConverter;
use deno_error::JsErrorBox;
use vello::peniko::Brush;
use vello::peniko::ColorStop;
use vello::peniko::ColorStops;
use vello::peniko::Gradient;
use vello::peniko::GradientKind;
use vello::peniko::color::AlphaColor;
use vello::peniko::color::DynamicColor;
use vello::peniko::kurbo::Point;

use super::state::parse_color;

#[derive(Clone, Copy)]
enum GradKind {
  Linear {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
  },
  Radial {
    x0: f64,
    y0: f64,
    r0: f64,
    x1: f64,
    y1: f64,
    r1: f64,
  },
  Conic {
    angle: f64,
    x: f64,
    y: f64,
  },
}

#[derive(Default, Clone)]
struct State {
  stops: Vec<ColorStop>,
}

pub struct CanvasGradient {
  kind: GradKind,
  state: RefCell<State>,
}

// SAFETY: holds no v8 references.
unsafe impl GarbageCollected for CanvasGradient {
  fn trace(&self, _visitor: &mut Visitor) {}
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"CanvasGradient"
  }
}

impl WebIdlInterfaceConverter for CanvasGradient {
  const NAME: &'static str = "CanvasGradient";
}

#[op2]
impl CanvasGradient {
  fn add_color_stop(
    &self,
    #[webidl] offset: UnrestrictedDouble,
    #[webidl] color: String,
  ) -> Result<(), JsErrorBox> {
    let off = *offset;
    if !off.is_finite() || !(0.0..=1.0).contains(&off) {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "offset out of range",
      ));
    }
    let Some(c) = parse_color(&color) else {
      return Err(JsErrorBox::type_error(format!("Invalid color: {color}")));
    };
    let _ = color;
    let mut s = self.state.borrow_mut();
    s.stops.push(ColorStop {
      offset: off as f32,
      color: DynamicColor::from_alpha_color::<vello::peniko::color::Srgb>(c),
    });
    s.stops.sort_by(|a, b| {
      a.offset
        .partial_cmp(&b.offset)
        .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(())
  }
}

impl CanvasGradient {
  pub fn new_linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
    Self {
      kind: GradKind::Linear { x0, y0, x1, y1 },
      state: Default::default(),
    }
  }

  pub fn new_radial(
    x0: f64,
    y0: f64,
    r0: f64,
    x1: f64,
    y1: f64,
    r1: f64,
  ) -> Result<Self, JsErrorBox> {
    if r0 < 0.0 || r1 < 0.0 {
      return Err(JsErrorBox::new(
        "DOMExceptionIndexSizeError",
        "Negative radius",
      ));
    }
    Ok(Self {
      kind: GradKind::Radial {
        x0,
        y0,
        r0,
        x1,
        y1,
        r1,
      },
      state: Default::default(),
    })
  }

  pub fn new_conic(angle: f64, x: f64, y: f64) -> Self {
    Self {
      kind: GradKind::Conic { angle, x, y },
      state: Default::default(),
    }
  }

  /// Build a peniko brush snapshot of the current gradient. Returns a solid
  /// transparent brush if no stops have been added (the safest fallback).
  pub fn to_brush(&self) -> Brush {
    let stops = self.state.borrow().stops.clone();
    if stops.is_empty() {
      return Brush::Solid(AlphaColor::TRANSPARENT);
    }
    let kind = match self.kind {
      GradKind::Linear { x0, y0, x1, y1 } => GradientKind::Linear {
        start: Point::new(x0, y0),
        end: Point::new(x1, y1),
      },
      GradKind::Radial {
        x0,
        y0,
        r0,
        x1,
        y1,
        r1,
      } => GradientKind::Radial {
        start_center: Point::new(x0, y0),
        start_radius: r0 as f32,
        end_center: Point::new(x1, y1),
        end_radius: r1 as f32,
      },
      GradKind::Conic { angle, x, y } => GradientKind::Sweep {
        center: Point::new(x, y),
        start_angle: angle as f32,
        end_angle: (angle + std::f64::consts::TAU) as f32,
      },
    };
    let mut g = Gradient {
      kind,
      ..Default::default()
    };
    g.stops = ColorStops(stops.into_iter().collect());
    Brush::Gradient(g)
  }
}
