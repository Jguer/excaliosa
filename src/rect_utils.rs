use crate::models::ExcalidrawElement;

// Excalidraw roundness constants
pub const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
pub const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;

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

