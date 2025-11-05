/// Color parsing and element color/stroke detection utilities
use crate::models::ExcalidrawElement;

/// Parse a hex color string into RGBA components
/// Accepts:
/// - "transparent" => (0, 0, 0, 0)
/// - #RRGGBB or RRGGBB => (r, g, b, 255)
/// - #RRGGBBAA or RRGGBBAA => (r, g, b, a)
/// - Empty string => (0, 0, 0, 0)
/// - Invalid format => (0, 0, 0, 255) - defaults to black
pub fn parse_color(color_str: &str) -> (u8, u8, u8, u8) {
    if color_str.eq_ignore_ascii_case("transparent") || color_str.is_empty() {
        return (0, 0, 0, 0);
    }
    
    let trimmed = color_str.trim();
    let hex = if let Some(rest) = trimmed.strip_prefix('#') {
        rest
    } else {
        trimmed
    };
    
    match hex.len() {
        6 => {
            // RRGGBB format
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                (r, g, b, 255)
            } else {
                (0, 0, 0, 255) // Default to black on parse error
            }
        }
        8 => {
            // RRGGBBAA format
            if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
                u8::from_str_radix(&hex[6..8], 16),
            ) {
                (r, g, b, a)
            } else {
                (0, 0, 0, 255) // Default to black on parse error
            }
        }
        _ => {
            // Invalid format - default to black
            (0, 0, 0, 255)
        }
    }
}

/// Parse a hex color string into RGBA with Result type (for error handling)
/// Used when we need to propagate errors (e.g., CLI argument parsing)
pub fn parse_color_result(color_str: &str) -> Result<(u8, u8, u8, u8), String> {
    if color_str.eq_ignore_ascii_case("transparent") {
        return Ok((0, 0, 0, 0));
    }
    
    let trimmed = color_str.trim();
    let hex = if let Some(rest) = trimmed.strip_prefix('#') {
        rest
    } else {
        trimmed
    };
    
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16)
                .map_err(|_| "Invalid hex digit in R component")?;
            let g = u8::from_str_radix(&hex[2..4], 16)
                .map_err(|_| "Invalid hex digit in G component")?;
            let b = u8::from_str_radix(&hex[4..6], 16)
                .map_err(|_| "Invalid hex digit in B component")?;
            Ok((r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16)
                .map_err(|_| "Invalid hex digit in R component")?;
            let g = u8::from_str_radix(&hex[2..4], 16)
                .map_err(|_| "Invalid hex digit in G component")?;
            let b = u8::from_str_radix(&hex[4..6], 16)
                .map_err(|_| "Invalid hex digit in B component")?;
            let a = u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| "Invalid hex digit in A component")?;
            Ok((r, g, b, a))
        }
        _ => Err(format!(
            "Expected 6 or 8 hex digits (RRGGBB or RRGGBBAA), got {}",
            hex.len()
        )),
    }
}

/// Check if a color string represents transparency
pub fn is_transparent(color: &str) -> bool {
    color.eq_ignore_ascii_case("transparent") || color.is_empty()
}

/// Determine if an element should render a stroke
/// Returns true if:
/// - stroke_color is not empty
/// - stroke_color is not "transparent"
/// - stroke_width > 0.0
pub fn has_stroke(element: &ExcalidrawElement) -> bool {
    !element.stroke_color.is_empty()
        && !is_transparent(&element.stroke_color)
        && element.stroke_width > 0.0
}

/// Determine if an element should render a fill
/// Returns true if:
/// - background_color is not empty
/// - background_color is not "transparent"
pub fn has_fill(element: &ExcalidrawElement) -> bool {
    !element.background_color.is_empty()
        && !is_transparent(&element.background_color)
}

