use crate::models::{ExcalidrawData, ExcalidrawElement, ViewBox};

pub fn calculate_viewbox(elements: &[ExcalidrawElement]) -> ViewBox {
    const PADDING: f64 = 40.0;

    if elements.is_empty() {
        return ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: 800.0,
            height: 600.0,
        };
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for el in elements {
        if !el.isDeleted {
            min_x = min_x.min(el.x);
            min_y = min_y.min(el.y);
            max_x = max_x.max(el.x + el.width);
            max_y = max_y.max(el.y + el.height);
        }
    }

    ViewBox {
        min_x: min_x - PADDING,
        min_y: min_y - PADDING,
        width: max_x - min_x + PADDING * 2.0,
        height: max_y - min_y + PADDING * 2.0,
    }
}

fn get_font_family(font_id: Option<i32>) -> &'static str {
    match font_id {
        Some(1) => "Virgil, Segoe UI Emoji",
        Some(2) => "Cascadia, monospace",
        _ => "Virgil, Segoe UI Emoji",
    }
}

fn get_stroke_dasharray(stroke_style: &str) -> &'static str {
    match stroke_style {
        "dashed" => "8,4",
        "dotted" => "2,2",
        _ => "none",
    }
}

fn get_text_anchor(text_align: Option<&str>) -> &'static str {
    match text_align {
        Some("center") => "middle",
        Some("right") => "end",
        Some("left") => "start",
        _ => "start",
    }
}

fn get_vertical_offset(vertical_align: Option<&str>, font_size: f64) -> f64 {
    match vertical_align {
        Some("middle") => font_size * 0.35,
        Some("bottom") => font_size * 0.9,
        _ => font_size * 0.75,
    }
}

fn render_element(el: &ExcalidrawElement, _viewbox: &ViewBox) -> String {
    if el.isDeleted {
        return String::new();
    }

    let stroke_color = if el.strokeColor.is_empty() {
        "#000000"
    } else {
        &el.strokeColor
    };

    let background_color = if el.backgroundColor.is_empty() || el.backgroundColor == "transparent" {
        "none"
    } else {
        &el.backgroundColor
    };

    let opacity = el.opacity / 100.0;
    let transform = format!(
        "rotate({} {} {})",
        el.angle,
        el.x + el.width / 2.0,
        el.y + el.height / 2.0
    );
    
    let stroke_dasharray = get_stroke_dasharray(&el.strokeStyle);
    let dasharray_attr = if stroke_dasharray != "none" {
        format!(r#" stroke-dasharray="{}""#, stroke_dasharray)
    } else {
        String::new()
    };

    match el.element_type.as_str() {
        "rectangle" => {
            let rx = if el.roundness.is_some() { 8.0 } else { 0.0 };
            format!(
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}"/>"#,
                el.x, el.y, el.width, el.height, rx, background_color, stroke_color, el.strokeWidth, opacity, dasharray_attr, transform
            )
        }
        "diamond" => {
            let points = [
                (el.x + el.width / 2.0, el.y),
                (el.x + el.width, el.y + el.height / 2.0),
                (el.x + el.width / 2.0, el.y + el.height),
                (el.x, el.y + el.height / 2.0),
            ];
            let points_str = points
                .iter()
                .map(|(x, y)| format!("{},{}", x, y))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                r#"<polygon points="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}"/>"#,
                points_str, background_color, stroke_color, el.strokeWidth, opacity, dasharray_attr, transform
            )
        }
        "ellipse" => {
            format!(
                r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}"/>"#,
                el.x + el.width / 2.0,
                el.y + el.height / 2.0,
                el.width / 2.0,
                el.height / 2.0,
                background_color,
                stroke_color,
                el.strokeWidth,
                opacity,
                dasharray_attr,
                transform
            )
        }
        "line" | "arrow" => {
            if let Some(ref points) = el.points {
                if !points.is_empty() {
                    let path_data = format!(
                        "M {}",
                        points
                            .iter()
                            .map(|(px, py)| format!("{},{}", el.x + px, el.y + py))
                            .collect::<Vec<_>>()
                            .join(" L ")
                    );
                    
                    // Check for endArrowhead (new field) or endArrowType (legacy)
                    let has_arrow = el.endArrowhead.is_some() || el.endArrowType.is_some();
                    let marker_end = if has_arrow {
                        r#" marker-end="url(#arrowhead)""#
                    } else {
                        ""
                    };
                    
                    return format!(
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}"{}{} transform="{}"/>"#,
                        path_data, stroke_color, el.strokeWidth, opacity, dasharray_attr, marker_end, transform
                    );
                }
            }
            String::new()
        }
        "text" => {
            let font_size = el.fontSize.unwrap_or(16.0);
            let text = el.text.as_deref().unwrap_or("");
            let font_family = get_font_family(el.fontFamily);
            let y_offset = if el.verticalAlign.as_deref() == Some("middle") {
                el.y + el.height / 2.0
            } else if el.verticalAlign.as_deref() == Some("bottom") {
                el.y + el.height
            } else {
                el.y + get_vertical_offset(el.verticalAlign.as_deref(), font_size)
            };
            
            let x_pos = if el.textAlign.as_deref() == Some("center") {
                el.x + el.width / 2.0
            } else if el.textAlign.as_deref() == Some("right") {
                el.x + el.width
            } else {
                el.x
            };
            
            let alignment_anchor = get_text_anchor(el.textAlign.as_deref());
            let dy_attr = if el.verticalAlign.as_deref() == Some("middle") {
                r#" dominant-baseline="middle""#
            } else {
                ""
            };
            
            format!(
                r#"<text x="{}" y="{}" font-size="{}" font-family="{}" fill="{}" opacity="{}" text-anchor="{}"{} transform="{}">{}</text>"#,
                x_pos,
                y_offset,
                font_size,
                font_family,
                stroke_color,
                opacity,
                alignment_anchor,
                dy_attr,
                transform,
                escape_xml(text)
            )
        }
        _ => String::new(),
    }
}

fn escape_xml(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&apos;")
}

pub fn generate_svg(data: &ExcalidrawData) -> String {
    let viewbox = calculate_viewbox(&data.elements);

    let elements_svg = data
        .elements
        .iter()
        .map(|el| render_element(el, &viewbox))
        .collect::<Vec<_>>()
        .join("\n");

    let fill_color = "#000000";
    format!(
        "<svg viewBox=\"{} {} {} {}\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n  <defs>\n    <marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n      <polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\"/>\n    </marker>\n  </defs>\n  {}\n</svg>",
        viewbox.min_x, viewbox.min_y, viewbox.width, viewbox.height, fill_color, elements_svg
    )
}
