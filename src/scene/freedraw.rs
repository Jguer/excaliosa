//! Freehand ink lowered to a filled offset outline and an optional rough fill.
//!
//! shortcut: speed-based pressure, exponential streamline smoothing, and
//! polygonal round caps approximate perfect-freehand/LaserPointer; upgrade
//! those algorithms only for pixel parity.

use roughr::generator::Generator;

use crate::color_utils::{fill_color, has_stroke, parse_color};
use crate::models::ExcalidrawElement;

use super::build::{lower_drawable, rough_options, to_points};
use super::{PaintedPath, PathOp, Rgba};

const VARIABLE_SIZE_FACTOR: f64 = 4.25;
const CONSTANT_SIZE_FACTOR: f64 = 1.4;
const THINNING: f64 = 0.6;
const DEFAULT_STREAMLINE: f64 = 0.5;
const CAP_SEGMENTS: usize = 12;
const DOT_SEGMENTS: usize = 32;

#[derive(Clone, Copy)]
struct Sample {
    point: (f64, f64),
    pressure: f64,
}

pub(super) fn build_freedraw(
    generator: &Generator,
    el: &ExcalidrawElement,
) -> Option<Vec<PaintedPath>> {
    let points = absolute_points(el)?;
    let mut out = Vec::new();

    if is_loop(&points)
        && let Some(fill) = fill_color(el).map(Rgba::from_tuple)
    {
        let opts = rough_options(el, false, true, false);
        let drawable = generator.curve::<f64>(&to_points(&points), &Some(opts));
        out.extend(lower_drawable(
            &drawable,
            None,
            Some(fill),
            el.stroke_width,
            None,
        ));
    }

    if has_stroke(el)
        && let Some(outline) = outline(el)
    {
        let mut ops = Vec::with_capacity(outline.len() + 2);
        ops.push(PathOp::Move(outline[0].0, outline[0].1));
        ops.extend(outline.iter().skip(1).map(|&(x, y)| PathOp::Line(x, y)));
        ops.push(PathOp::Close);
        out.push(PaintedPath {
            ops,
            fill: Some(Rgba::from_tuple(parse_color(&el.stroke_color))),
            stroke: None,
            stroke_width: 0.0,
            dash: None,
            fill_even_odd: false,
        });
    }

    (!out.is_empty()).then_some(out)
}

/// The saved freehand point extrema define Excalidraw's rotation pivot.
pub(crate) fn point_center(el: &ExcalidrawElement) -> Option<(f64, f64)> {
    let points = absolute_points(el)?;
    let (min_x, min_y, max_x, max_y) = extents(points.iter().copied())?;
    Some(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

/// Absolute outline vertices, also used to keep painted ink inside the viewport.
pub(crate) fn outline(el: &ExcalidrawElement) -> Option<Vec<(f64, f64)>> {
    let constant = is_constant_width(el);
    let factor = if constant {
        CONSTANT_SIZE_FACTOR
    } else {
        VARIABLE_SIZE_FACTOR
    };
    let size = el.stroke_width * factor;
    if !size.is_finite() || size <= 0.0 {
        return None;
    }

    let mut samples = input_samples(el, size, constant)?;
    if samples.is_empty() {
        return None;
    }
    smooth_samples(&mut samples, el);

    let radius = |pressure: f64| {
        if constant {
            size / 2.0
        } else {
            let eased = (pressure * std::f64::consts::FRAC_PI_2).sin();
            size / 2.0 * (1.0 - THINNING * (1.0 - eased))
        }
    };

    if samples.len() == 1 {
        return Some(circle_points(samples[0].point, radius(samples[0].pressure)));
    }

    let mut left = Vec::with_capacity(samples.len());
    let mut right = Vec::with_capacity(samples.len());
    for (i, sample) in samples.iter().enumerate() {
        let previous = samples[i.saturating_sub(1)].point;
        let next = samples[(i + 1).min(samples.len() - 1)].point;
        let (tx, ty) = unit_vector(previous, next)?;
        let normal = (-ty, tx);
        let r = radius(sample.pressure);
        left.push((sample.point.0 + normal.0 * r, sample.point.1 + normal.1 * r));
        right.push((sample.point.0 - normal.0 * r, sample.point.1 - normal.1 * r));
    }

    let start_tangent = unit_vector(samples[0].point, samples[1].point)?;
    let end_tangent = unit_vector(
        samples[samples.len() - 2].point,
        samples[samples.len() - 1].point,
    )?;
    let mut outline = left;
    append_cap(
        &mut outline,
        samples[samples.len() - 1].point,
        end_tangent,
        radius(samples[samples.len() - 1].pressure),
        false,
    );
    outline.extend(right.iter().rev().skip(1));
    append_cap(
        &mut outline,
        samples[0].point,
        start_tangent,
        radius(samples[0].pressure),
        true,
    );
    Some(outline)
}

fn input_samples(el: &ExcalidrawElement, size: f64, constant: bool) -> Option<Vec<Sample>> {
    let points = el.points.as_ref()?;
    let indexed: Vec<(usize, (f64, f64))> = points
        .iter()
        .copied()
        .enumerate()
        .map(|(i, (x, y))| (i, (el.x + x, el.y + y)))
        .filter(|(_, (x, y))| x.is_finite() && y.is_finite())
        .collect();
    if indexed.is_empty() {
        return None;
    }

    let recorded = (!constant && !el.simulate_pressure)
        .then_some(el.pressures.as_ref())
        .flatten()
        .filter(|pressures| {
            pressures.len() >= points.len()
                && pressures.iter().take(points.len()).all(|p| p.is_finite())
        });
    let simulated = (!constant && recorded.is_none()).then(|| simulated_pressures(&indexed, size));

    let mut samples: Vec<Sample> = Vec::with_capacity(indexed.len());
    for (i, point) in indexed {
        let pressure = if constant {
            1.0
        } else if let Some(pressures) = recorded {
            pressures[i].clamp(0.0, 1.0)
        } else {
            simulated
                .as_ref()
                .and_then(|p| p.get(i))
                .copied()
                .unwrap_or(0.5)
        };
        if let Some(previous) = samples.last_mut()
            && distance(previous.point, point) <= 1e-9
        {
            previous.pressure = pressure;
            continue;
        }
        samples.push(Sample { point, pressure });
    }
    Some(samples)
}

fn simulated_pressures(points: &[(usize, (f64, f64))], size: f64) -> Vec<f64> {
    let mut result = Vec::with_capacity(points.len());
    let mut pressure = 0.5;
    for (i, &(_, point)) in points.iter().enumerate() {
        if i > 0 {
            let speed = distance(points[i - 1].1, point) / size.max(1.0);
            let target = (1.0 - speed).clamp(0.0, 1.0);
            pressure += (target - pressure) * 0.35;
        }
        result.push(pressure.clamp(0.0, 1.0));
    }
    result
}

fn smooth_samples(samples: &mut [Sample], el: &ExcalidrawElement) {
    if samples.len() < 3 {
        return;
    }
    let streamline = el
        .stroke_options
        .as_ref()
        .and_then(|options| options.streamline)
        .filter(|value| value.is_finite())
        .unwrap_or(DEFAULT_STREAMLINE)
        .clamp(0.0, 1.0);
    let alpha = 1.0 - streamline * 0.95;
    let original = samples.to_vec();
    let mut previous = original[0];
    for i in 1..samples.len() - 1 {
        let current = original[i];
        samples[i].point = (
            previous.point.0 + (current.point.0 - previous.point.0) * alpha,
            previous.point.1 + (current.point.1 - previous.point.1) * alpha,
        );
        samples[i].pressure = previous.pressure + (current.pressure - previous.pressure) * alpha;
        previous = samples[i];
    }
}

fn append_cap(
    outline: &mut Vec<(f64, f64)>,
    center: (f64, f64),
    tangent: (f64, f64),
    radius: f64,
    start: bool,
) {
    let normal = if start {
        (tangent.1, -tangent.0)
    } else {
        (-tangent.1, tangent.0)
    };
    let direction = if start {
        (-tangent.0, -tangent.1)
    } else {
        tangent
    };
    let range = if start {
        1..CAP_SEGMENTS
    } else {
        1..CAP_SEGMENTS + 1
    };
    for step in range {
        let angle = std::f64::consts::PI * step as f64 / CAP_SEGMENTS as f64;
        let (sin, cos) = angle.sin_cos();
        outline.push((
            center.0 + radius * (normal.0 * cos + direction.0 * sin),
            center.1 + radius * (normal.1 * cos + direction.1 * sin),
        ));
    }
}

fn circle_points(center: (f64, f64), radius: f64) -> Vec<(f64, f64)> {
    (0..DOT_SEGMENTS)
        .map(|step| {
            let angle = std::f64::consts::TAU * step as f64 / DOT_SEGMENTS as f64;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect()
}

fn is_constant_width(el: &ExcalidrawElement) -> bool {
    el.stroke_options
        .as_ref()
        .and_then(|options| options.variability.as_deref())
        == Some("constant")
}

fn absolute_points(el: &ExcalidrawElement) -> Option<Vec<(f64, f64)>> {
    let points: Vec<(f64, f64)> = el
        .points
        .as_ref()?
        .iter()
        .map(|&(x, y)| (el.x + x, el.y + y))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    (!points.is_empty()).then_some(points)
}

fn is_loop(points: &[(f64, f64)]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let distinct: Vec<(f64, f64)> = points.iter().copied().fold(Vec::new(), |mut out, p| {
        if out
            .last()
            .is_none_or(|previous| distance(*previous, p) > 1e-9)
        {
            out.push(p);
        }
        out
    });
    distinct.len() >= 3 && distance(points[0], points[points.len() - 1]) <= 8.0
}

fn unit_vector(from: (f64, f64), to: (f64, f64)) -> Option<(f64, f64)> {
    let length = distance(from, to);
    (length > 1e-9).then_some(((to.0 - from.0) / length, (to.1 - from.1) / length))
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

fn extents(points: impl Iterator<Item = (f64, f64)>) -> Option<(f64, f64, f64, f64)> {
    let mut result = None;
    for (x, y) in points {
        let (min_x, min_y, max_x, max_y) = result.get_or_insert((
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ));
        *min_x = min_x.min(x);
        *min_y = min_y.min(y);
        *max_x = max_x.max(x);
        *max_y = max_y.max(y);
    }
    result
}
