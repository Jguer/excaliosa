//! SVG backend: serializes a [`Scene`] to an SVG document.
//!
//! Shape geometry comes from the same scene the raster backend uses, so `.svg`
//! export matches the PNG output. Shapes are emitted as `<path>` elements (the
//! roughr op-lists), text as `<text>`/`<tspan>`, and images as `<image>`.

use crate::models::ExcalidrawData;
use crate::scene::{
    build_scene, Drawable, ImagePrimitive, PaintedPath, PathOp, Primitive, Rgba, TextAlign,
    TextPrimitive,
};

/// Generate an SVG document string for the given Excalidraw data.
pub fn generate_svg(data: &ExcalidrawData, background: Option<(u8, u8, u8, u8)>) -> String {
    let scene = build_scene(data, background.map(Rgba::from_tuple));
    let vb = scene.viewbox;

    let mut body = String::new();
    if let Some(bg) = scene.background {
        if bg.3 > 0 {
            body.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"{:.4}\"/>\n",
                vb.min_x,
                vb.min_y,
                vb.width,
                vb.height,
                hex(bg),
                bg.3 as f64 / 255.0
            ));
        }
    }

    for drawable in &scene.drawables {
        let rendered = render_drawable(drawable);
        if !rendered.is_empty() {
            body.push_str(&rendered);
            body.push('\n');
        }
    }

    format!(
        "<svg viewBox=\"{} {} {} {}\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n{}</svg>",
        vb.min_x, vb.min_y, vb.width, vb.height, body
    )
}

fn render_drawable(drawable: &Drawable) -> String {
    let inner = match &drawable.primitive {
        Primitive::Paths(paths) => paths.iter().map(path_svg).collect::<Vec<_>>().join("\n"),
        Primitive::Text(text) => text_svg(text),
        Primitive::Image(image) => image_svg(image),
    };
    if inner.is_empty() {
        return String::new();
    }

    // Fold element opacity and rotation into a wrapping group so overlapping
    // rough passes composite as one layer (matching the raster backend).
    let mut attrs = String::new();
    if (drawable.opacity - 1.0).abs() > 1e-9 {
        attrs.push_str(&format!(" opacity=\"{:.4}\"", drawable.opacity));
    }
    if drawable.rotation_deg != 0.0 {
        attrs.push_str(&format!(
            " transform=\"rotate({} {} {})\"",
            drawable.rotation_deg, drawable.center.0, drawable.center.1
        ));
    }

    if attrs.is_empty() {
        inner
    } else {
        format!("<g{attrs}>\n{inner}\n</g>")
    }
}

fn path_svg(pp: &PaintedPath) -> String {
    let fill = pp.fill.map(hex).unwrap_or_else(|| "none".to_string());
    let stroke = pp.stroke.map(hex).unwrap_or_else(|| "none".to_string());

    let mut s = format!("<path d=\"{}\" fill=\"{}\" stroke=\"{}\"", ops_to_d(&pp.ops), fill, stroke);

    if let Some(c) = pp.fill {
        if c.3 < 255 {
            s.push_str(&format!(" fill-opacity=\"{:.4}\"", c.3 as f64 / 255.0));
        }
        if pp.fill_even_odd {
            s.push_str(" fill-rule=\"evenodd\"");
        }
    }
    if pp.stroke.is_some() && pp.stroke_width > 0.0 {
        s.push_str(&format!(" stroke-width=\"{}\"", pp.stroke_width));
    }
    if let Some(c) = pp.stroke {
        if c.3 < 255 {
            s.push_str(&format!(" stroke-opacity=\"{:.4}\"", c.3 as f64 / 255.0));
        }
    }
    if let Some(dash) = &pp.dash {
        let pattern = dash.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        s.push_str(&format!(" stroke-dasharray=\"{pattern}\""));
    }
    if pp.stroke.is_some() {
        s.push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
    }
    s.push_str("/>");
    s
}

fn text_svg(t: &TextPrimitive) -> String {
    let (anchor, anchor_x) = match t.align {
        TextAlign::Left => ("start", t.x),
        TextAlign::Center => ("middle", t.x + t.container_width / 2.0),
        TextAlign::Right => ("end", t.x + t.container_width),
    };

    let tspans: String = t
        .content
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            let y = t.y + t.baseline_offset + i as f64 * t.line_height;
            format!(
                "<tspan x=\"{anchor_x:.2}\" y=\"{y:.2}\" style=\"white-space: pre;\">{}</tspan>",
                escape_xml(line)
            )
        })
        .collect();

    format!(
        "<text font-size=\"{}\" font-family=\"{}\" fill=\"{}\" fill-opacity=\"{:.4}\" text-anchor=\"{}\" dominant-baseline=\"alphabetic\">{}</text>",
        t.font_size,
        t.font_family,
        hex(t.color),
        t.color.3 as f64 / 255.0,
        anchor,
        tspans
    )
}

fn image_svg(im: &ImagePrimitive) -> String {
    // Represent axis flips as a local transform about the image center.
    let mut transform = String::new();
    if im.flip_x || im.flip_y {
        let sx = if im.flip_x { -1.0 } else { 1.0 };
        let sy = if im.flip_y { -1.0 } else { 1.0 };
        let tx = if im.flip_x { 2.0 * im.x + im.width } else { 0.0 };
        let ty = if im.flip_y { 2.0 * im.y + im.height } else { 0.0 };
        transform = format!(" transform=\"translate({tx} {ty}) scale({sx} {sy})\"");
    }
    format!(
        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"none\"{}/>",
        im.x,
        im.y,
        im.width,
        im.height,
        escape_xml(&im.data_url),
        transform
    )
}

fn ops_to_d(ops: &[PathOp]) -> String {
    ops.iter()
        .map(|op| match *op {
            PathOp::Move(x, y) => format!("M {x:.2} {y:.2}"),
            PathOp::Line(x, y) => format!("L {x:.2} {y:.2}"),
            PathOp::Cubic(a, b, c, d, e, f) => {
                format!("C {a:.2} {b:.2}, {c:.2} {d:.2}, {e:.2} {f:.2}")
            }
            PathOp::Close => "Z".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
