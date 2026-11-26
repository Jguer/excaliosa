/// Color parsing and element color/stroke detection utilities
use crate::models::ExcalidrawElement;

/// Parse a hex color string into RGBA components
/// Accepts:
/// - "transparent" => (0, 0, 0, 0)
/// - #RGB or #RGBA => each digit is doubled
/// - #RRGGBB or RRGGBB => (r, g, b, 255)
/// - #RRGGBBAA or RRGGBBAA => (r, g, b, a)
/// - Empty string => (0, 0, 0, 0)
/// - Invalid format => (0, 0, 0, 255) - defaults to black
pub fn parse_color(color_str: &str) -> (u8, u8, u8, u8) {
    // Empty means "no color" (fully transparent); anything else defers to the
    // fallible parser, defaulting to opaque black on malformed input.
    if color_str.is_empty() {
        return (0, 0, 0, 0);
    }
    parse_hex_color(color_str, true).unwrap_or((0, 0, 0, 255))
}

/// Parse a hex color string into RGBA with Result type (for error handling)
/// Used when we need to propagate errors (e.g., CLI argument parsing)
pub fn parse_color_result(color_str: &str) -> Result<(u8, u8, u8, u8), String> {
    parse_hex_color(color_str, false)
}

fn parse_hex_color(color_str: &str, allow_shorthand: bool) -> Result<(u8, u8, u8, u8), String> {
    if color_str.eq_ignore_ascii_case("transparent") {
        return Ok((0, 0, 0, 0));
    }

    let trimmed = color_str.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let shorthand = allow_shorthand && trimmed.starts_with('#') && matches!(hex.len(), 3 | 4);
    if !shorthand && !matches!(hex.len(), 6 | 8) {
        return Err(format!(
            "Expected 6 or 8 hex digits (RRGGBB or RRGGBBAA), got {}",
            hex.len()
        ));
    }
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid hex digit".into());
    }

    let bytes = u32::from_str_radix(hex, 16)
        .map_err(|e| e.to_string())?
        .to_be_bytes();
    let expand = |digit| digit * 17;
    Ok(match (hex.len(), bytes) {
        (3, [_, _, high, low]) => (expand(high), expand(low >> 4), expand(low & 0xf), 255),
        (4, [_, _, high, low]) => (
            expand(high >> 4),
            expand(high & 0xf),
            expand(low >> 4),
            expand(low & 0xf),
        ),
        (6, [_, r, g, b]) => (r, g, b, 255),
        (_, [r, g, b, a]) => (r, g, b, a),
    })
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

/// Determine if an element has a valid, visible fill color.
pub fn has_fill(element: &ExcalidrawElement) -> bool {
    fill_color(element).is_some()
}

pub(crate) fn fill_color(element: &ExcalidrawElement) -> Option<(u8, u8, u8, u8)> {
    parse_hex_color(&element.background_color, true)
        .ok()
        .filter(|color| color.3 > 0)
}
