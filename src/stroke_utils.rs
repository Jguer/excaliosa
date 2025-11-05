/// Compute stroke dash pattern based on Excalidraw's strokeStyle and strokeWidth
/// Returns dash array as Vec<f64> or None for solid strokes
pub fn get_stroke_dash_array(stroke_style: &str, stroke_width: f64) -> Option<Vec<f64>> {
    match stroke_style {
        "dashed" => {
            // [8, 8 + strokeWidth]
            Some(vec![8.0, 8.0 + stroke_width.max(0.0)])
        }
        "dotted" => {
            // [1.5, 6 + strokeWidth]
            Some(vec![1.5, 6.0 + stroke_width.max(0.0)])
        }
        _ => None,
    }
}

/// Dotted dash pattern for arrow caps uses strokeWidth-1
pub fn get_dotted_cap_dash_array(stroke_width: f64) -> Vec<f64> {
    let adj = (stroke_width - 1.0).max(0.0);
    vec![1.5, 6.0 + adj]
}

/// Get stroke dash array as SVG attribute value (e.g., "8,12")
/// Returns "none" for solid strokes (for use in stroke-dasharray attribute)
pub fn get_stroke_dasharray_attr(stroke_style: &str, stroke_width: f64) -> String {
    get_stroke_dash_array(stroke_style, stroke_width)
        .map(|dash| {
            dash.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "none".to_string())
}

