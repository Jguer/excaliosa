/// Shared utilities for arrow and arrowhead rendering.This module provides common logic used by both SVG and Skia renderers
use crate::math_utils::catmull_rom_cubics;

pub const CARDINALITY_ONE_OR_MANY_BAR_OFFSET: f64 = -0.25;
pub const CARDINALITY_INNER_BAR_OFFSET: f64 = -0.5;
pub const CARDINALITY_ZERO_CIRCLE_OFFSET: f64 = 1.5;
pub const CARDINALITY_ZERO_CIRCLE_SCALE: f64 = 0.8;

/// Normalize names used by older Excalidraw documents.
pub fn normalize_arrowhead(arrowhead: &str) -> &str {
    match arrowhead {
        "dot" => "circle",
        "crowfoot_one" => "cardinality_one",
        "crowfoot_many" => "cardinality_many",
        "crowfoot_one_or_many" => "cardinality_one_or_many",
        _ => arrowhead,
    }
}

/// Get arrowhead size based on arrowhead type (in Excalidraw units).
pub fn get_arrowhead_size(arrowhead: &str) -> f64 {
    match normalize_arrowhead(arrowhead) {
        "arrow" => 25.0,
        "diamond" | "diamond_outline" => 12.0,
        "cardinality_many" | "cardinality_one_or_many" | "cardinality_zero_or_many" => 15.0,
        "cardinality_one" | "cardinality_exactly_one" | "cardinality_zero_or_one" => 20.0,
        _ => 15.0,
    }
}

/// Get arrowhead angle in degrees based on arrowhead type.
pub fn get_arrowhead_angle(arrowhead: &str) -> f64 {
    get_arrowhead_angle_for_size(arrowhead, get_arrowhead_size(arrowhead))
}

pub fn get_arrowhead_angle_for_size(arrowhead: &str, size: f64) -> f64 {
    match normalize_arrowhead(arrowhead) {
        "bar" => 90.0,
        "arrow" => {
            let full_size = get_arrowhead_size("arrow");
            let shrinkage = ((full_size - size) / (full_size - 10.0)).clamp(0.0, 1.0);
            20.0 + 10.0 * shrinkage
        }
        _ => 25.0,
    }
}

#[derive(Clone, Copy)]
struct ArrowheadCorner {
    length: f64,
    previous_length: f64,
    turn: f64,
}

fn arrowhead_corner(
    points: &[(f64, f64)],
    position: &str,
    elbowed: bool,
) -> Option<ArrowheadCorner> {
    if points.len() < 3 {
        return None;
    }
    let (tip, corner, previous) = if position == "start" {
        (points[0], points[1], points[2])
    } else {
        let n = points.len();
        (points[n - 1], points[n - 2], points[n - 3])
    };
    let distance = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
    let mut length = distance(tip, corner);
    let mut previous_length = distance(corner, previous);
    if length == 0.0 || previous_length == 0.0 {
        return None;
    }
    if elbowed {
        let rounding = 16.0_f64.min(length / 2.0).min(previous_length / 2.0) / 2.0;
        length -= rounding;
        previous_length -= rounding;
    }
    let incoming = (tip.0 - corner.0, tip.1 - corner.1);
    let outgoing = (corner.0 - previous.0, corner.1 - previous.1);
    let cosine = ((incoming.0 * outgoing.0 + incoming.1 * outgoing.1) / (length * previous_length))
        .clamp(-1.0, 1.0);
    Some(ArrowheadCorner {
        length,
        previous_length,
        turn: cosine.acos(),
    })
}

fn sides_clear_size(corner: ArrowheadCorner, side_angle: f64, clearance: f64) -> f64 {
    let angle = side_angle.to_radians();
    if corner.turn <= angle {
        return f64::INFINITY;
    }
    (corner.length * corner.turn.sin() - clearance) / (corner.turn - angle).sin()
}

fn cardinality_marker_scale(marker: &str, corner: ArrowheadCorner, stroke_width: f64) -> f64 {
    let bar = |offset: f64| {
        let angle = get_arrowhead_angle("cardinality_one").to_radians();
        corner.length
            / (get_arrowhead_size("cardinality_one")
                * (offset + angle.cos() + angle.sin() * (corner.turn / 2.0).tan()))
    };
    let crowfoot = || corner.length / get_arrowhead_size("cardinality_many");
    let circle = || {
        (corner.length
            - ((stroke_width - 2.0) * CARDINALITY_ZERO_CIRCLE_SCALE) / 2.0
            - stroke_width)
            / (get_arrowhead_size("circle_outline")
                * (CARDINALITY_ZERO_CIRCLE_OFFSET + CARDINALITY_ZERO_CIRCLE_SCALE / 2.0))
    };
    match marker {
        "cardinality_one" => 1.0_f64.min(bar(0.0)),
        "cardinality_many" => 1.0_f64.min(crowfoot()),
        "cardinality_one_or_many" => 1.0_f64
            .min(crowfoot())
            .min(bar(CARDINALITY_ONE_OR_MANY_BAR_OFFSET)),
        "cardinality_exactly_one" => 1.0_f64.min(bar(CARDINALITY_INNER_BAR_OFFSET)).min(bar(0.0)),
        "cardinality_zero_or_one" => 1.0_f64.min(circle()).min(bar(CARDINALITY_INNER_BAR_OFFSET)),
        "cardinality_zero_or_many" => 1.0_f64.min(crowfoot()).min(circle()),
        _ => 1.0,
    }
}

/// Resolve the cap size, including clearance around a sharp preceding corner.
#[allow(clippy::too_many_arguments)]
pub fn resolve_arrowhead_size(
    points: &[(f64, f64)],
    position: &str,
    arrowhead: &str,
    marker: &str,
    stroke_width: f64,
    curved: bool,
    elbowed: bool,
) -> f64 {
    let arrowhead = normalize_arrowhead(arrowhead);
    let marker = normalize_arrowhead(marker);
    let size = get_arrowhead_size(arrowhead);
    let length = if position == "start" {
        (points[0].0 - points[1].0).hypot(points[0].1 - points[1].1)
    } else {
        let n = points.len();
        (points[n - 1].0 - points[n - 2].0).hypot(points[n - 1].1 - points[n - 2].1)
    };
    let fraction = if matches!(arrowhead, "diamond" | "diamond_outline") {
        0.25
    } else {
        0.5
    };
    let minimum = size.min(length * fraction);
    if minimum == size {
        return size;
    }
    let Some(corner) = arrowhead_corner(points, position, elbowed) else {
        return minimum;
    };
    let maximum = (corner.length + corner.previous_length) / 2.0;
    let beyond = if marker.starts_with("cardinality_") {
        if curved {
            0.0
        } else {
            size * cardinality_marker_scale(marker, corner, stroke_width)
        }
    } else if matches!(arrowhead, "circle" | "circle_outline") {
        maximum
    } else if curved {
        0.0
    } else {
        match arrowhead {
            "bar" => maximum.min(corner.length / (corner.turn / 2.0).tan()),
            "diamond" | "diamond_outline" => corner.length / 2.0,
            "triangle" | "triangle_outline" => maximum.min(sides_clear_size(
                corner,
                get_arrowhead_angle(arrowhead),
                0.0,
            )),
            "arrow" => {
                let legible = size / 2.0;
                legible
                    .min(maximum)
                    .min(sides_clear_size(
                        corner,
                        get_arrowhead_angle_for_size(arrowhead, legible),
                        stroke_width,
                    ))
                    .min(sides_clear_size(
                        corner,
                        get_arrowhead_angle_for_size(arrowhead, 0.0),
                        stroke_width,
                    ))
            }
            _ => 0.0,
        }
    };
    size.min(minimum.max(beyond))
}

/// Rotate a point around a center point by a given angle (in radians).
pub fn rotate_point<T>(px: T, py: T, cx: T, cy: T, angle_rad: T) -> (T, T)
where
    T: num_traits::Float,
{
    let dx = px - cx;
    let dy = py - cy;
    let ca = angle_rad.cos();
    let sa = angle_rad.sin();
    (cx + dx * ca - dy * sa, cy + dx * sa + dy * ca)
}

/// Calculate cubic Bezier point at parameter t
pub fn cubic_point<T>(p0: (T, T), p1: (T, T), p2: (T, T), p3: (T, T), t: T) -> (T, T)
where
    T: num_traits::Float,
{
    let u = T::one() - t;
    let u2 = u * u;
    let u3 = u2 * u;
    let t2 = t * t;
    let t3 = t2 * t;
    let x = u3 * p0.0 + T::from(3.0).unwrap() * u2 * t * p1.0 + T::from(3.0).unwrap() * u * t2 * p2.0 + t3 * p3.0;
    let y = u3 * p0.1 + T::from(3.0).unwrap() * u2 * t * p1.1 + T::from(3.0).unwrap() * u * t2 * p2.1 + t3 * p3.1;
    (x, y)
}

/// Calculate arrowhead points without preceding-corner information.
#[allow(clippy::too_many_arguments)]
pub fn calc_arrowhead_points<T>(
    x_tail: T,
    y_tail: T,
    x_tip: T,
    y_tip: T,
    arrowhead: &str,
    stroke_width: T,
    segment_length: T,
) -> Vec<T>
where
    T: num_traits::Float,
{
    let fraction = if matches!(
        normalize_arrowhead(arrowhead),
        "diamond" | "diamond_outline"
    ) {
        T::from(0.25).unwrap()
    } else {
        T::from(0.5).unwrap()
    };
    let size = T::from(get_arrowhead_size(arrowhead))
        .unwrap()
        .min((segment_length * fraction).max(T::zero()));
    let points = calc_arrowhead_points_at_size(
        x_tail,
        y_tail,
        x_tip,
        y_tip,
        arrowhead,
        stroke_width,
        size,
        T::zero(),
    );
    if normalize_arrowhead(arrowhead) == "bar" && points.len() == 6 {
        points[2..].to_vec()
    } else {
        points
    }
}

/// Calculate arrowhead points at a resolved size and fractional tip offset.
/// Circles return center and diameter; forks return their base and two flanks;
/// other caps return tip and two flanks, with diamonds adding their back tip.
#[allow(clippy::too_many_arguments)]
pub fn calc_arrowhead_points_at_size<T>(
    x_tail: T,
    y_tail: T,
    x_tip: T,
    y_tip: T,
    arrowhead: &str,
    stroke_width: T,
    size: T,
    offset_multiplier: T,
) -> Vec<T>
where
    T: num_traits::Float,
{
    let dx = x_tip - x_tail;
    let dy = y_tip - y_tail;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance == T::zero() {
        return vec![];
    }
    let nx = dx / distance;
    let ny = dy / distance;
    let tx = x_tip - nx * size * offset_multiplier;
    let ty = y_tip - ny * size * offset_multiplier;
    let xs = tx - nx * size;
    let ys = ty - ny * size;
    let arrowhead = normalize_arrowhead(arrowhead);

    if matches!(arrowhead, "circle" | "circle_outline") {
        let diameter = size + stroke_width - T::from(2.0).unwrap();
        return if diameter > T::zero() {
            vec![tx, ty, diameter]
        } else {
            vec![]
        };
    }

    let angle = T::from(get_arrowhead_angle_for_size(
        arrowhead,
        size.to_f64().unwrap_or(0.0),
    ))
    .unwrap()
    .to_radians();
    if matches!(arrowhead, "cardinality_many" | "cardinality_one_or_many") {
        let (x3, y3) = rotate_point(tx, ty, xs, ys, -angle);
        let (x4, y4) = rotate_point(tx, ty, xs, ys, angle);
        return vec![xs, ys, x3, y3, x4, y4];
    }

    let (x3, y3) = rotate_point(xs, ys, tx, ty, -angle);
    let (x4, y4) = rotate_point(xs, ys, tx, ty, angle);
    if matches!(arrowhead, "diamond" | "diamond_outline") {
        let two = T::one() + T::one();
        vec![
            tx,
            ty,
            x3,
            y3,
            tx - nx * size * two,
            ty - ny * size * two,
            x4,
            y4,
        ]
    } else {
        vec![tx, ty, x3, y3, x4, y4]
    }
}

/// Build elbow arrow path with rounded corners
///
/// Takes absolute points and returns an SVG path string with rounded corners.
/// Converts quadratic curves to cubic Bezier for compatibility.
///
/// # Arguments
/// * `points` - Absolute coordinates [(x, y), ...]
/// * `max_corner` - Maximum corner radius (typically 16.0)
///
/// # Returns
/// SVG path string starting with "M" command, or None if insufficient points
pub fn build_elbow_arrow_path(points: &[(f64, f64)], max_corner: f64) -> Option<String> {
    if points.len() < 2 {
        return None;
    }
    if points.len() == 2 {
        let start = points[0];
        let end = points[1];
        return Some(format!("M {} {} L {} {}", start.0, start.1, end.0, end.1));
    }

    // Helper: check if movement is primarily horizontal
    fn is_horizontal(p: (f64, f64), prev: (f64, f64)) -> bool {
        (p.0 - prev.0).abs() >= (p.1 - prev.1).abs()
    }

    // Helper: 2D distance
    fn dist2d(a: (f64, f64), b: (f64, f64)) -> f64 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    }

    // Build sub-commands: for each middle point, push L, Q control, Q end
    let mut sub: Vec<(f64, f64)> = Vec::new();
    for i in 1..(points.len() - 1) {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];
        let prev_is_h = is_horizontal(curr, prev);
        let next_is_h = is_horizontal(next, curr);
        let corner = max_corner
            .min(dist2d(curr, next) * 0.5)
            .min(dist2d(prev, curr) * 0.5);

        // last point before corner
        if prev_is_h {
            if prev.0 < curr.0 {
                sub.push((curr.0 - corner, curr.1));
            } else {
                sub.push((curr.0 + corner, curr.1));
            }
        } else if prev.1 < curr.1 {
            sub.push((curr.0, curr.1 - corner));
        } else {
            sub.push((curr.0, curr.1 + corner));
        }

        // corner control point
        sub.push((curr.0, curr.1));

        // next segment start after the corner
        if next_is_h {
            if next.0 < curr.0 {
                sub.push((curr.0 - corner, curr.1));
            } else {
                sub.push((curr.0 + corner, curr.1));
            }
        } else if next.1 < curr.1 {
            sub.push((curr.0, curr.1 - corner));
        } else {
            sub.push((curr.0, curr.1 + corner));
        }
    }

    let start = points[0];
    let mut d = format!("M {} {}", start.0, start.1);
    for chunk in sub.chunks(3) {
        if let [l, q1, q2] = chunk {
            // Quadratic control q1 => cubic c1,c2
            let c1 = (
                l.0 + (2.0 / 3.0) * (q1.0 - l.0),
                l.1 + (2.0 / 3.0) * (q1.1 - l.1),
            );
            let c2 = (
                q2.0 + (2.0 / 3.0) * (q1.0 - q2.0),
                q2.1 + (2.0 / 3.0) * (q1.1 - q2.1),
            );
            d.push_str(&format!(" L {} {}", l.0, l.1));
            d.push_str(&format!(
                " C {} {}, {} {}, {} {}",
                c1.0, c1.1, c2.0, c2.1, q2.0, q2.1
            ));
        }
    }
    let end = points[points.len() - 1];
    d.push_str(&format!(" L {} {}", end.0, end.1));
    Some(d)
}

/// Calculate arrowhead direction from curve segments using Catmull-Rom cubics
/// Returns (tail_x, tail_y, tip_x, tip_y, segment_length) for arrowhead calculation
///
/// # Arguments
/// * `points` - Relative points from element (will be converted to absolute with x, y offset)
/// * `x` - X offset to convert relative to absolute coordinates
/// * `y` - Y offset to convert relative to absolute coordinates
/// * `position` - "start" or "end" to determine which arrowhead to calculate
/// * `tension` - Catmull-Rom tension parameter (typically 0.5)
pub fn calculate_arrowhead_direction<T>(
    points: &[(T, T)],
    x: T,
    y: T,
    position: &str,
    tension: T,
) -> Option<(T, T, T, T, T)>
where
    T: num_traits::Float + Copy,
{
    if points.is_empty() {
        return None;
    }

    // Convert relative points to absolute
    let abs_points: Vec<(T, T)> = points.iter().map(|(px, py)| (x + *px, y + *py)).collect();

    let cubics = catmull_rom_cubics(&abs_points, tension);
    if cubics.is_empty() {
        return None;
    }

    let (p0, cp1, cp2, p3) = if position == "start" {
        cubics[0]
    } else {
        *cubics.last().unwrap()
    };

    // Tip is at the endpoint of the segment
    let (tip_x, tip_y) = if position == "start" {
        (p0.0, p0.1)
    } else {
        (p3.0, p3.1)
    };

    // Point near tip for direction calculation (use curve tangent)
    let t = if position == "start" {
        T::from(0.3).unwrap()
    } else {
        T::from(0.7).unwrap()
    };
    let (tail_x, tail_y) = cubic_point(p0, cp1, cp2, p3, t);

    // Calculate segment length from element points (local, relative)
    let seg_len = if position == "end" {
        if points.len() >= 2 {
            let a = points[points.len() - 1];
            let b = points[points.len() - 2];
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
        } else {
            T::zero()
        }
    } else if points.len() >= 2 {
        let a = points[0];
        let b = points[1];
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    } else {
        T::zero()
    };

    Some((tail_x, tail_y, tip_x, tip_y, seg_len))
}
