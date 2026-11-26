use crate::models::ExcalidrawElement;

// Excalidraw roundness constants
pub const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
pub const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;

// Bound recursive curve subdivision and keep its squared flatness arithmetic finite.
const MAX_SAFE_CURVE_VALUE: f64 = 1_000_000.0;

// Roundness types
pub const ROUNDNESS_LEGACY: i32 = 1;
pub const ROUNDNESS_PROPORTIONAL_RADIUS: i32 = 2;
pub const ROUNDNESS_ADAPTIVE_RADIUS: i32 = 3;

/// Calculate corner radius based on Excalidraw's roundness algorithm
/// x: dimension (typically min(width, height) for rectangles)
/// element: the element with roundness data
pub fn get_corner_radius(x: f64, element: &ExcalidrawElement) -> f64 {
    if let Some(ref roundness) = element.roundness {
        match roundness.roundness_type {
            ROUNDNESS_PROPORTIONAL_RADIUS | ROUNDNESS_LEGACY => {
                return x * DEFAULT_PROPORTIONAL_RADIUS;
            }
            ROUNDNESS_ADAPTIVE_RADIUS => {
                let fixed_radius_size = roundness.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
                let cutoff_size = fixed_radius_size / DEFAULT_PROPORTIONAL_RADIUS;
                
                if x <= cutoff_size {
                    return x * DEFAULT_PROPORTIONAL_RADIUS;
                }
                return fixed_radius_size;
            }
            _ => return 0.0,
        }
    }
    0.0
}

/// Generate the trimmed diamond edges and cubic corners used by Excalidraw.
/// Returns `None` when rough curve subdivision would be unsafe.
pub fn generate_rounded_diamond_path(element: &ExcalidrawElement) -> Option<String> {
    let x = element.x;
    let y = element.y;
    let top_x = x + element.width / 2.0;
    let top_y = y;
    let right_x = x + element.width;
    let right_y = y + element.height / 2.0;
    let bottom_x = top_x;
    let bottom_y = y + element.height;
    let left_x = x;
    let left_y = right_y;
    let vertical_radius = get_corner_radius((top_x - left_x).abs(), element);
    let horizontal_radius = get_corner_radius((right_y - top_y).abs(), element);
    let coordinates = [
        top_x,
        top_y,
        right_x,
        right_y,
        bottom_x,
        bottom_y,
        left_x,
        left_y,
        vertical_radius,
        horizontal_radius,
    ];
    let spans = [element.width, element.height];
    let unsafe_value = |value: f64| !value.is_finite() || value.abs() > MAX_SAFE_CURVE_VALUE;
    if coordinates.into_iter().any(unsafe_value) || spans.into_iter().any(unsafe_value) {
        return None;
    }

    Some(format!(
        "M {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {}",
        top_x + vertical_radius,
        top_y + horizontal_radius,
        right_x - vertical_radius,
        right_y - horizontal_radius,
        right_x,
        right_y,
        right_x,
        right_y,
        right_x - vertical_radius,
        right_y + horizontal_radius,
        bottom_x + vertical_radius,
        bottom_y - horizontal_radius,
        bottom_x,
        bottom_y,
        bottom_x,
        bottom_y,
        bottom_x - vertical_radius,
        bottom_y - horizontal_radius,
        left_x + vertical_radius,
        left_y + horizontal_radius,
        left_x,
        left_y,
        left_x,
        left_y,
        left_x + vertical_radius,
        left_y - horizontal_radius,
        top_x - vertical_radius,
        top_y + horizontal_radius,
        top_x,
        top_y,
        top_x,
        top_y,
        top_x + vertical_radius,
        top_y + horizontal_radius,
    ))
}

/// Generate SVG path string for a rounded rectangle using quadratic curve commands
/// This uses Q commands (quadratic curves) which work well with Skia and provide smooth corners
pub fn generate_rounded_rect_path(x: f64, y: f64, width: f64, height: f64, radius: f64) -> String {
    let r = radius.min(width / 2.0).min(height / 2.0);
    if r <= 0.0 {
        return format!(
            "M {} {} L {} {} L {} {} L {} {} Z",
            x, y,
            x + width, y,
            x + width, y + height,
            x, y + height
        );
    }
    // Use quadratic curves (Q commands) for smooth rounded corners
    // M (x+r) y L (x+w-r) y Q (x+w) y, (x+w) (y+r) L (x+w) (y+h-r)
    // Q (x+w) (y+h), (x+w-r) (y+h) L (x+r) (y+h) Q x (y+h), x (y+h-r)
    // L x (y+r) Q x y, (x+r) y
    format!(
        "M {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {}",
        x + r, y,
        x + width - r, y,
        x + width, y, x + width, y + r,
        x + width, y + height - r,
        x + width, y + height, x + width - r, y + height,
        x + r, y + height,
        x, y + height, x, y + height - r,
        x, y + r,
        x, y, x + r, y
    )
}

