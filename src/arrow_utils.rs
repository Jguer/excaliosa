/// Shared utilities for arrow and arrowhead rendering.This module provides common logic used by both SVG and Skia renderers
/// Get arrowhead size based on arrowhead type (in Excalidraw units)
pub fn get_arrowhead_size(arrowhead: &str) -> f64 {
    match arrowhead {
        "arrow" => 25.0,
        "diamond" | "diamond_outline" => 12.0,
        "crowfoot_many" | "crowfoot_one" | "crowfoot_one_or_many" => 20.0,
        "dot" | "circle" | "circle_outline" => 15.0,
        "bar" => 15.0,
        "triangle" | "triangle_outline" => 15.0,
        _ => 15.0,
    }
}

/// Get arrowhead angle in degrees based on arrowhead type
pub fn get_arrowhead_angle(arrowhead: &str) -> f64 {
    match arrowhead {
        "bar" => 90.0,
        "arrow" => 20.0,
        _ => 25.0,
    }
}

/// Rotate a point around a center point by a given angle (in radians)
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

/// Calculate arrowhead points from tip and tail coordinates
/// Returns a vector of values representing the arrowhead geometry:
/// - For circles: [center_x, center_y, diameter]
/// - For triangles/arrows: [tip_x, tip_y, side1_x, side1_y, side2_x, side2_y]
/// - For diamonds: [tip_x, tip_y, side1_x, side1_y, opposite_x, opposite_y, side2_x, side2_y]
/// - For crowfoot: [base_x, base_y, side1_x, side1_y, side2_x, side2_y]
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
    let dx = x_tip - x_tail;
    let dy = y_tip - y_tail;
    let dist_sq = dx * dx + dy * dy;
    let dist = dist_sq.sqrt();
    
    if dist == T::zero() {
        return vec![];
    }

    // Normalized direction vector (from tail to tip)
    let nx = dx / dist;
    let ny = dy / dist;

    let base_size = T::from(get_arrowhead_size(arrowhead)).unwrap();
    
    // Scale with strokeWidth like Excalidraw
    let size_multiplier = T::one() + (stroke_width - T::one()) * T::from(0.3).unwrap();
    
    // Scale down for short segments
    let length_mult = if arrowhead == "diamond" || arrowhead == "diamond_outline" {
        T::from(0.25).unwrap()
    } else {
        T::from(0.5).unwrap()
    };
    let min_size = (base_size * size_multiplier).min(segment_length * length_mult);

    // Point on shaft where arrowhead base starts
    let xs = x_tip - nx * min_size;
    let ys = y_tip - ny * min_size;

    match arrowhead {
        "dot" | "circle" | "circle_outline" => {
            // Return [center_x, center_y, diameter]
            let diameter = ((ys - y_tip).powi(2) + (xs - x_tip).powi(2)).sqrt() + stroke_width - T::from(2.0).unwrap();
            vec![x_tip, y_tip, diameter]
        }
        "bar" => {
            // Perpendicular bar
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let x3 = xs + (-ny * cos_a - nx * sin_a) * min_size;
            let y3 = ys + (nx * cos_a - ny * sin_a) * min_size;
            let x4 = xs + (-ny * cos_a + nx * sin_a) * min_size;
            let y4 = ys + (nx * cos_a + ny * sin_a) * min_size;
            vec![x3, y3, x4, y4]
        }
        "arrow" => {
            // Open arrow (two lines)
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            // Rotate backwards direction by +/- angle
            let x3 = x_tip + (-nx * cos_a - ny * sin_a) * min_size;
            let y3 = y_tip + (-ny * cos_a + nx * sin_a) * min_size;
            let x4 = x_tip + (-nx * cos_a + ny * sin_a) * min_size;
            let y4 = y_tip + (-ny * cos_a - nx * sin_a) * min_size;
            vec![x_tip, y_tip, x3, y3, x4, y4]
        }
        "triangle" | "triangle_outline" => {
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let x3 = xs + (-ny * cos_a - nx * sin_a) * min_size;
            let y3 = ys + (nx * cos_a - ny * sin_a) * min_size;
            let x4 = xs + (-ny * cos_a + nx * sin_a) * min_size;
            let y4 = ys + (nx * cos_a + ny * sin_a) * min_size;
            vec![x_tip, y_tip, x3, y3, x4, y4]
        }
        "diamond" | "diamond_outline" => {
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let x3 = xs + (-ny * cos_a - nx * sin_a) * min_size;
            let y3 = ys + (nx * cos_a - ny * sin_a) * min_size;
            let x4 = xs + (-ny * cos_a + nx * sin_a) * min_size;
            let y4 = ys + (nx * cos_a + ny * sin_a) * min_size;
            // Point opposite to tip
            let ox = x_tip - nx * min_size * T::from(2.0).unwrap();
            let oy = y_tip - ny * min_size * T::from(2.0).unwrap();
            vec![x_tip, y_tip, x3, y3, ox, oy, x4, y4]
        }
        "crowfoot_many" | "crowfoot_one_or_many" => {
            // swap (xs,ys) with (x_tip,y_tip) and rotate around (xs,ys)
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let (x3, y3) = rotate_point(x_tip, y_tip, xs, ys, -angle);
            let (x4, y4) = rotate_point(x_tip, y_tip, xs, ys, angle);
            vec![xs, ys, x3, y3, x4, y4]
        }
        "crowfoot_one" => {
            // Similar to crowfoot_many but different rendering
            let angle = T::from(get_arrowhead_angle(arrowhead)).unwrap().to_radians();
            let (x3, y3) = rotate_point(x_tip, y_tip, xs, ys, -angle);
            let (x4, y4) = rotate_point(x_tip, y_tip, xs, ys, angle);
            vec![xs, ys, x3, y3, x4, y4]
        }
        _ => vec![],
    }
}

