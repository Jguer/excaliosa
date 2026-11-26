use std::collections::HashMap;

use crate::math_utils::{CubicBezierSegment, calculate_center, catmull_rom_cubics};
use crate::models::ExcalidrawElement;

use super::LabelGap;

const LABEL_GAP_PADDING: f64 = 5.0;
const CURVE_SAMPLES: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedArrowLabel {
    pub text_index: usize,
    pub x: f64,
    pub y: f64,
    pub gap: LabelGap,
}

/// Resolve only unambiguous live arrow/text pairs. Labels without a usable
/// resolved position remain ordinary text at their saved coordinates.
pub(crate) fn resolve_arrow_labels(
    elements: &[ExcalidrawElement],
) -> HashMap<usize, ResolvedArrowLabel> {
    let mut arrows_by_id: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut text_id_counts: HashMap<&str, usize> = HashMap::new();
    let mut texts_by_container: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut arrows_by_text: HashMap<&str, Vec<usize>> = HashMap::new();

    for (index, element) in elements.iter().enumerate() {
        if element.is_deleted {
            continue;
        }
        match element.element_type.as_str() {
            "arrow" => {
                arrows_by_id.entry(&element.id).or_default().push(index);
                for bound in element.bound_elements.iter().flatten() {
                    if bound.element_type == "text" {
                        arrows_by_text.entry(&bound.id).or_default().push(index);
                    }
                }
            }
            "text" => {
                *text_id_counts.entry(&element.id).or_default() += 1;
                if let Some(container_id) = &element.container_id {
                    texts_by_container
                        .entry(container_id)
                        .or_default()
                        .push(index);
                }
            }
            _ => {}
        }
    }

    let mut resolved = HashMap::new();
    for (text_index, text) in elements.iter().enumerate() {
        if text.is_deleted
            || text.element_type != "text"
            || !text
                .text
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }

        let candidates = match &text.container_id {
            Some(container_id) => arrows_by_id.get(container_id.as_str()),
            None => arrows_by_text.get(text.id.as_str()),
        };
        let Some(&[arrow_index]) = candidates.map(Vec::as_slice) else {
            continue;
        };
        let arrow = &elements[arrow_index];
        let backlinks: Vec<&str> = arrow
            .bound_elements
            .iter()
            .flatten()
            .filter(|bound| bound.element_type == "text")
            .map(|bound| bound.id.as_str())
            .collect();
        let claims = arrows_by_text
            .get(text.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let binding_matches = match backlinks.as_slice() {
            [id] if *id == text.id => {
                text_id_counts.get(text.id.as_str()) == Some(&1) && claims == [arrow_index]
            }
            [] => {
                text.container_id.as_deref() == Some(arrow.id.as_str())
                    && texts_by_container
                        .get(arrow.id.as_str())
                        .is_some_and(|texts| texts.len() == 1)
                    && claims.is_empty()
            }
            _ => false,
        };
        if !binding_matches || resolved.contains_key(&arrow_index) {
            continue;
        }

        let Some(center) = label_center(arrow, text.label_position) else {
            continue;
        };
        let x = center.0 - text.width / 2.0;
        let y = center.1 - text.height / 2.0;
        resolved.insert(
            arrow_index,
            ResolvedArrowLabel {
                text_index,
                x,
                y,
                gap: LabelGap {
                    x: x - LABEL_GAP_PADDING,
                    y: y - LABEL_GAP_PADDING,
                    width: text.width + LABEL_GAP_PADDING * 2.0,
                    height: text.height + LABEL_GAP_PADDING * 2.0,
                },
            },
        );
    }
    resolved
}

fn label_center(arrow: &ExcalidrawElement, label_position: Option<f64>) -> Option<(f64, f64)> {
    let points = arrow.points.as_ref()?;
    if points.len() < 2 {
        return None;
    }
    let absolute: Vec<(f64, f64)> = points
        .iter()
        .map(|(x, y)| (arrow.x + x, arrow.y + y))
        .collect();
    let curved =
        !arrow.elbowed.unwrap_or(false) && arrow.roundness.is_some() && absolute.len() >= 3;
    let curves = curved.then(|| catmull_rom_cubics(&absolute, 0.5));

    let center = if let Some(position) = label_position.filter(|value| value.is_finite()) {
        point_at_path_fraction(&absolute, curves.as_deref(), position.clamp(0.0, 1.0))
    } else if absolute.len() % 2 == 1 {
        absolute[absolute.len() / 2]
    } else {
        let segment = absolute.len() / 2 - 1;
        point_at_segment_fraction(&absolute, curves.as_deref(), segment, 0.5)
    };

    // Match the renderer's stored-box pivot; deriving precise linear geometry
    // pivots is intentionally independent of bound-label placement.
    let pivot = calculate_center(arrow.x, arrow.y, arrow.width, arrow.height);
    let (sin, cos) = arrow.angle.sin_cos();
    let (dx, dy) = (center.0 - pivot.0, center.1 - pivot.1);
    Some((pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos))
}

fn point_at_path_fraction(
    points: &[(f64, f64)],
    curves: Option<&[CubicBezierSegment<f64>]>,
    fraction: f64,
) -> (f64, f64) {
    let lengths: Vec<f64> = (0..points.len() - 1)
        .map(|index| sampled_segment(points, curves, index, 1.0).1)
        .collect();
    let total: f64 = lengths.iter().sum();
    if total == 0.0 {
        return points[0];
    }
    let target = total * fraction;
    let mut traversed = 0.0;
    for (index, length) in lengths.iter().enumerate() {
        if target <= traversed + length || index + 1 == lengths.len() {
            let fraction = if *length == 0.0 {
                0.0
            } else {
                ((target - traversed) / length).clamp(0.0, 1.0)
            };
            return point_at_segment_fraction(points, curves, index, fraction);
        }
        traversed += length;
    }
    points[points.len() - 1]
}

fn point_at_segment_fraction(
    points: &[(f64, f64)],
    curves: Option<&[CubicBezierSegment<f64>]>,
    segment: usize,
    fraction: f64,
) -> (f64, f64) {
    sampled_segment(points, curves, segment, fraction).0
}

/// Sample cubics with a fixed budget, then interpolate by sampled arc length.
fn sampled_segment(
    points: &[(f64, f64)],
    curves: Option<&[CubicBezierSegment<f64>]>,
    segment: usize,
    fraction: f64,
) -> ((f64, f64), f64) {
    let Some(curves) = curves else {
        let start = points[segment];
        let end = points[segment + 1];
        let length = (end.0 - start.0).hypot(end.1 - start.1);
        return (
            (
                start.0 + (end.0 - start.0) * fraction,
                start.1 + (end.1 - start.1) * fraction,
            ),
            length,
        );
    };

    let curve = curves[segment];
    let samples: Vec<(f64, f64)> = (0..=CURVE_SAMPLES)
        .map(|index| cubic_point(curve, index as f64 / CURVE_SAMPLES as f64))
        .collect();
    let mut lengths = Vec::with_capacity(CURVE_SAMPLES);
    let mut total = 0.0;
    for pair in samples.windows(2) {
        let length = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        lengths.push(length);
        total += length;
    }
    let target = total * fraction;
    let mut traversed = 0.0;
    for (index, length) in lengths.iter().enumerate() {
        if target <= traversed + length || index + 1 == lengths.len() {
            let local = if *length == 0.0 {
                0.0
            } else {
                (target - traversed) / length
            };
            return (
                (
                    samples[index].0 + (samples[index + 1].0 - samples[index].0) * local,
                    samples[index].1 + (samples[index + 1].1 - samples[index].1) * local,
                ),
                total,
            );
        }
        traversed += length;
    }
    (samples[samples.len() - 1], total)
}

fn cubic_point(curve: CubicBezierSegment<f64>, t: f64) -> (f64, f64) {
    let (p0, p1, p2, p3) = curve;
    let u = 1.0 - t;
    let coordinate = |a: f64, b: f64, c: f64, d: f64| {
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
    };
    (
        coordinate(p0.0, p1.0, p2.0, p3.0),
        coordinate(p0.1, p1.1, p2.1, p3.1),
    )
}
