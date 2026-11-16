//! Timeline interpolation for layer transforms.
//!
//! [`transform_at`] is the entry point the composer calls once per layer and
//! frame; [`ease`] and [`interpolate`] expose the same math for direct use.

use crate::scene::{Easing, Layer, Transform};

/// Newton-Raphson passes used to invert the cubic-bezier x curve.
const NEWTON_ITERATIONS: usize = 8;
/// Bisection passes used when Newton-Raphson does not converge.
const BISECTION_ITERATIONS: usize = 32;
/// Accepted curve parameter error before a solver pass stops early.
const SOLUTION_TOLERANCE: f32 = 1e-6;
/// Derivative magnitude below which Newton-Raphson is unsafe.
const MIN_DERIVATIVE: f32 = 1e-6;

/// Evaluate an easing curve at normalized time `t` (clamped to 0..=1).
pub fn ease(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);

    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - 2.0 * (1.0 - t) * (1.0 - t)
            }
        }
        Easing::CubicBezier { x1, y1, x2, y2 } => {
            ease_cubic_bezier(x1, y1, x2, y2, t).clamp(0.0, 1.0)
        }
    }
}

/// Interpolate between two transforms with an already-eased `t`.
///
/// `x` and `y` are rounded to whole cells; `scale` and `opacity` keep their
/// fractional value.
pub fn interpolate(from: Transform, to: Transform, t: f32) -> Transform {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Transform {
        x: lerp(from.x as f32, to.x as f32).round() as i32,
        y: lerp(from.y as f32, to.y as f32).round() as i32,
        scale: lerp(from.scale, to.scale),
        opacity: lerp(from.opacity, to.opacity),
    }
}

/// Transform of `layer` at `frame`.
///
/// Frames before the first keyframe clamp to it, as do frames after the last
/// one. Frames between two keyframes interpolate with the source keyframe's
/// easing. Validation guarantees strictly increasing frames; malformed windows
/// are skipped defensively and an uncovered frame falls back to the base
/// transform.
pub fn transform_at(layer: &Layer, frame: u32) -> Transform {
    let (Some(first), Some(last)) = (layer.keyframes.first(), layer.keyframes.last()) else {
        return layer.transform;
    };

    if frame <= first.frame {
        return first.transform;
    }
    if frame >= last.frame {
        return last.transform;
    }

    for window in layer.keyframes.windows(2) {
        let (from, to) = (&window[0], &window[1]);
        if to.frame <= from.frame {
            continue;
        }
        if frame <= from.frame {
            return from.transform;
        }
        if frame >= to.frame {
            continue;
        }

        let progress = (frame - from.frame) as f32 / (to.frame - from.frame) as f32;
        return interpolate(from.transform, to.transform, ease(from.easing, progress));
    }

    layer.transform
}

fn ease_cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    // Validation rejects out-of-range controls; clamping keeps direct callers
    // on a well-defined curve.
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);
    let u = solve_bezier_x(x1, x2, t);

    bezier_axis(y1, y2, u)
}

/// Invert `x(u) = t` for the unit cubic bezier with x controls `x1` and `x2`.
fn solve_bezier_x(x1: f32, x2: f32, t: f32) -> f32 {
    let mut u = t;

    for _ in 0..NEWTON_ITERATIONS {
        let error = bezier_axis(x1, x2, u) - t;
        if error.abs() < SOLUTION_TOLERANCE {
            return u;
        }
        let derivative = bezier_axis_derivative(x1, x2, u);
        if derivative.abs() < MIN_DERIVATIVE {
            break;
        }
        let next = u - error / derivative;
        if !next.is_finite() || !(0.0..=1.0).contains(&next) {
            break;
        }
        u = next;
    }

    if (bezier_axis(x1, x2, u) - t).abs() < SOLUTION_TOLERANCE {
        u
    } else {
        solve_bezier_x_bisection(x1, x2, t)
    }
}

fn solve_bezier_x_bisection(x1: f32, x2: f32, t: f32) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;

    for _ in 0..BISECTION_ITERATIONS {
        let middle = (low + high) * 0.5;
        if bezier_axis(x1, x2, middle) < t {
            low = middle;
        } else {
            high = middle;
        }
    }

    (low + high) * 0.5
}

/// Unit cubic bezier with endpoints 0 and 1.
fn bezier_axis(control1: f32, control2: f32, u: f32) -> f32 {
    let inverse = 1.0 - u;
    3.0 * inverse * inverse * u * control1 + 3.0 * inverse * u * u * control2 + u * u * u
}

fn bezier_axis_derivative(control1: f32, control2: f32, u: f32) -> f32 {
    let inverse = 1.0 - u;
    3.0 * inverse * inverse * control1
        + 6.0 * inverse * u * (control2 - control1)
        + 3.0 * u * u * (1.0 - control2)
}
