/// Map Excalidraw font IDs to their bundled equivalents.
pub fn get_font_family(font_id: Option<i32>) -> &'static str {
    match font_id {
        Some(2 | 9) => "Liberation Sans", // Helvetica or Liberation Sans
        Some(3) => "Cascadia Code",
        _ => "Excalifont", // ID 5, legacy Virgil (1), or any unavailable font
    }
}

/// Line height and alphabetic baseline measured from the top of a line box.
pub(crate) fn text_metrics(family: &str, size: f64, line_height: Option<f64>) -> (f64, f64) {
    // From the bundled fonts' head/hhea tables; center the font in the line box.
    let (units_per_em, ascent, descent, default_line_height) = match family {
        "Liberation Sans" => (2048.0, 1854.0, 434.0, 1.15),
        "Cascadia Code" => (2048.0, 1900.0, 480.0, 1.2),
        _ => (1000.0, 886.0, 374.0, 1.25),
    };
    let height = line_height.unwrap_or(default_line_height) * size;
    let baseline = (height + size * (ascent - descent) / units_per_em) / 2.0;
    (height, baseline)
}

/// Get SVG text-anchor attribute value based on text alignment
/// Maps Excalidraw text alignment to SVG text-anchor values
pub fn get_svg_text_anchor(text_align: Option<&str>) -> &'static str {
    match text_align {
        Some("center") => "middle",
        Some("right") => "end",
        Some("left") => "start",
        _ => "start",
    }
}

/// Calculate absolute X position for text based on alignment
/// Used for SVG rendering where text-anchor handles alignment
/// 
/// # Arguments
/// * `x` - Left edge of text container
/// * `width` - Width of text container
/// * `text_align` - Text alignment ("left", "center", "right", or None)
/// 
/// # Returns
/// Absolute X coordinate for text positioning
pub fn calculate_text_x_position<T>(x: T, width: T, text_align: Option<&str>) -> T
where
    T: num_traits::Float,
{
    match text_align {
        Some("center") => x + width / T::from(2.0).unwrap(),
        Some("right") => x + width,
        _ => x, // "left" or default
    }
}

/// Calculate starting X position for text line accounting for actual rendered width
/// Used for Skia/Pixel rendering where we need to account for actual text width
/// 
/// # Arguments
/// * `x` - Left edge of text container
/// * `container_width` - Width of text container
/// * `line_width` - Actual rendered width of the text line
/// * `text_align` - Text alignment ("left", "center", "right", or None)
/// 
/// # Returns
/// Starting X coordinate for rendering the text line
pub fn calculate_text_x_position_for_line<T>(
    x: T,
    container_width: T,
    line_width: T,
    text_align: Option<&str>,
) -> T
where
    T: num_traits::Float,
{
    match text_align {
        Some("center") => x + (container_width - line_width) / T::from(2.0).unwrap(),
        Some("right") => x + container_width - line_width,
        _ => x, // "left" or default
    }
}

/// Calculate vertical offset for text baseline based on vertical alignment
/// 
/// # Arguments
/// * `vertical_align` - Vertical alignment ("top", "middle", "bottom", or None)
/// * `font_size` - Font size in pixels
/// 
/// # Returns
/// Vertical offset from top of text container
pub fn get_vertical_offset<T>(vertical_align: Option<&str>, font_size: T) -> T
where
    T: num_traits::Float,
{
    match vertical_align {
        Some("middle") => font_size * T::from(0.35).unwrap(),
        Some("bottom") => font_size * T::from(0.9).unwrap(),
        _ => font_size * T::from(0.75).unwrap(), // "top" or default
    }
}

/// Calculate line height based on font size and optional line height multiplier
/// 
/// # Arguments
/// * `font_size` - Font size in pixels
/// * `line_height` - Optional line height multiplier (defaults to 1.25)
/// 
/// # Returns
/// Line height in pixels
pub fn get_line_height<T>(font_size: T, line_height: Option<T>) -> T
where
    T: num_traits::Float,
{
    line_height.unwrap_or(T::from(1.25).unwrap()) * font_size
}

