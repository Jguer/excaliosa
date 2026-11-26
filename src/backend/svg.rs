//! SVG backend: serializes a [`Scene`] to an SVG document.
//!
//! Shape geometry comes from the same scene the raster backend uses, so `.svg`
//! export matches the PNG output. Shapes are emitted as `<path>` elements (the
//! roughr op-lists), text as `<text>`/`<tspan>`, and images as `<image>`.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use base64::Engine;

use crate::font_utils::{normalize_newlines, text_direction};
use crate::fonts::bundled_font;
use crate::models::ExcalidrawData;
use crate::scene::{
    Drawable, ImagePrimitive, PaintedPath, PathOp, Primitive, Rgba, TextAlign, TextPrimitive,
    build_scene,
};

/// Generate an SVG document string for the given Excalidraw data.
pub fn generate_svg(data: &ExcalidrawData, background: Option<(u8, u8, u8, u8)>) -> String {
    let scene = build_scene(data, background.map(Rgba::from_tuple));
    let vb = scene.viewbox;
    let fonts = font_css(&scene.drawables);

    let mut body = String::new();
    let mut definitions = String::new();
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

    for (index, drawable) in scene.drawables.iter().enumerate() {
        let rendered = render_drawable(drawable, index, &mut definitions);
        if !rendered.is_empty() {
            body.push_str(&rendered);
            body.push('\n');
        }
    }
    let definitions = if definitions.is_empty() {
        definitions
    } else {
        format!("  <defs>\n{definitions}  </defs>\n")
    };

    format!(
        "<svg viewBox=\"{} {} {} {}\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n{}{}{}</svg>",
        vb.min_x, vb.min_y, vb.width, vb.height, fonts, definitions, body
    )
}

fn font_css(drawables: &[Drawable]) -> String {
    let used: BTreeSet<&str> = drawables
        .iter()
        .filter_map(|drawable| match &drawable.primitive {
            Primitive::Text(text) => Some(text.font_family),
            _ => None,
        })
        .filter(|family| bundled_font(family).embed_in_svg)
        .collect();
    if used.is_empty() {
        return String::new();
    }

    let mut css = String::from("  <defs>\n    <style class=\"style-fonts\">\n");
    for family in used {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bundled_font(family).bytes);
        css.push_str(&format!(
            "      @font-face {{ font-family: \"{family}\"; src: url(\"data:font/ttf;base64,{encoded}\") format(\"truetype\"); }}\n"
        ));
    }
    css.push_str("    </style>\n  </defs>\n");
    css
}

fn render_drawable(drawable: &Drawable, index: usize, definitions: &mut String) -> String {
    let separate_path_opacity = drawable
        .frame_opacity
        .is_some_and(|opacity| (opacity - 1.0).abs() > 1e-9)
        && matches!(&drawable.primitive, Primitive::Paths(_));
    let inner = match &drawable.primitive {
        Primitive::Paths(paths) => paths
            .iter()
            .map(|path| {
                path_svg(
                    path,
                    if separate_path_opacity {
                        drawable.opacity
                    } else {
                        1.0
                    },
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Primitive::Text(text) => text_svg(text),
        Primitive::Image(image) => image_svg(image, index),
    };
    if inner.is_empty() {
        return String::new();
    }

    // Fold ordinary element opacity and rotation into a wrapping group so overlapping
    // rough passes composite as one layer. Frame-faded paths use separate paint alpha
    // to match tiny-skia's fill/stroke compositing.
    let mut attrs = String::new();
    if !separate_path_opacity && (drawable.opacity - 1.0).abs() > 1e-9 {
        attrs.push_str(&format!(" opacity=\"{:.4}\"", drawable.opacity));
    }
    if drawable.rotation_deg != 0.0 {
        attrs.push_str(&format!(
            " transform=\"rotate({} {} {})\"",
            drawable.rotation_deg, drawable.center.0, drawable.center.1
        ));
    }

    let rendered = if attrs.is_empty() {
        inner
    } else {
        format!("<g{attrs}>\n{inner}\n</g>")
    };
    let rendered = if let Some(clip) = &drawable.frame_clip {
        let id = format!("excaliosa-frame-clip-{index}");
        let transform = if clip.rotation_deg == 0.0 {
            String::new()
        } else {
            format!(
                " transform=\"rotate({} {} {})\"",
                clip.rotation_deg, clip.center.0, clip.center.1
            )
        };
        definitions.push_str(&format!(
            "    <clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\"{transform}/></clipPath>\n",
            ops_to_d(&clip.ops)
        ));
        format!("<g clip-path=\"url(#{id})\">\n{rendered}\n</g>")
    } else {
        rendered
    };

    if drawable.opacity == 0.0 {
        return rendered;
    }

    match drawable.link.as_deref().and_then(safe_link_href) {
        Some(href) => format!("<a href=\"{}\">\n{rendered}\n</a>", escape_xml(href)),
        None => rendered,
    }
}

fn safe_link_href(link: &str) -> Option<&str> {
    if link.is_empty()
        || link.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '\u{fffe}' | '\u{ffff}' | '\\' | '"' | '\'' | '<' | '>')
        })
    {
        return None;
    }

    if let Some(fragment) = link.strip_prefix('#') {
        return (!fragment.is_empty() && !fragment.contains('#')).then_some(link);
    }

    for scheme in ["https:", "http:"] {
        if let Some(rest) = strip_scheme(link, scheme) {
            return valid_http_url(rest).then_some(link);
        }
    }

    let address = strip_scheme(link, "mailto:")?;
    let (local, domain) = address.split_once('@')?;
    (!local.is_empty()
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
        && !domain.contains('@')
        && valid_host(domain))
    .then_some(link)
}

fn strip_scheme<'a>(link: &'a str, scheme: &str) -> Option<&'a str> {
    link.get(..scheme.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(scheme))
        .map(|_| &link[scheme.len()..])
}

fn valid_http_url(rest: &str) -> bool {
    let Some(rest) = rest.strip_prefix("//") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        Some(_) => return false,
        None => (authority, None),
    };
    valid_host(host)
        && port.is_none_or(|port| {
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok()
        })
}

fn valid_host(host: &str) -> bool {
    if host.is_empty()
        || !host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return false;
    }

    let last = host.rsplit('.').next().unwrap_or_default();
    let numeric = last.bytes().all(|b| b.is_ascii_digit())
        || (last.len() >= 2
            && last[..2].eq_ignore_ascii_case("0x")
            && last[2..].bytes().all(|b| b.is_ascii_hexdigit()));
    !numeric || host.parse::<Ipv4Addr>().is_ok()
}

fn path_svg(pp: &PaintedPath, opacity: f64) -> String {
    let fill = pp.fill.map(hex).unwrap_or_else(|| "none".to_string());
    let stroke = pp.stroke.map(hex).unwrap_or_else(|| "none".to_string());

    let mut s = format!(
        "<path d=\"{}\" fill=\"{}\" stroke=\"{}\"",
        ops_to_d(&pp.ops),
        fill,
        stroke
    );

    if let Some(c) = pp.fill {
        let alpha = c.3 as f64 / 255.0 * opacity;
        if alpha < 1.0 {
            s.push_str(&format!(" fill-opacity=\"{alpha:.4}\""));
        }
        if pp.fill_even_odd {
            s.push_str(" fill-rule=\"evenodd\"");
        }
    }
    if pp.stroke.is_some() && pp.stroke_width > 0.0 {
        s.push_str(&format!(" stroke-width=\"{}\"", pp.stroke_width));
    }
    if let Some(c) = pp.stroke {
        let alpha = c.3 as f64 / 255.0 * opacity;
        if alpha < 1.0 {
            s.push_str(&format!(" stroke-opacity=\"{alpha:.4}\""));
        }
    }
    if let Some(dash) = &pp.dash {
        let pattern = dash
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",");
        s.push_str(&format!(" stroke-dasharray=\"{pattern}\""));
    }
    if pp.stroke.is_some() {
        s.push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
    }
    s.push_str("/>");
    s
}

fn text_svg(t: &TextPrimitive) -> String {
    let direction = text_direction(&t.content);
    let (anchor, anchor_x) = match t.align {
        TextAlign::Left => (if direction == "rtl" { "end" } else { "start" }, t.x),
        TextAlign::Center => ("middle", t.x + t.container_width / 2.0),
        TextAlign::Right => (
            if direction == "rtl" { "start" } else { "end" },
            t.x + t.container_width,
        ),
    };

    let content = normalize_newlines(&t.content);
    let tspans: String = content
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
    let font = bundled_font(t.font_family);

    format!(
        "<text font-size=\"{}\" font-family=\"{}, {}\" fill=\"{}\" fill-opacity=\"{:.4}\" text-anchor=\"{}\" direction=\"{}\" dominant-baseline=\"alphabetic\">{}</text>",
        t.font_size,
        t.font_family,
        font.generic_family,
        hex(t.color),
        t.color.3 as f64 / 255.0,
        anchor,
        direction,
        tspans
    )
}

fn image_svg(im: &ImagePrimitive, index: usize) -> String {
    let (x, y, width, height) = if let Some(crop) = im.crop {
        let uncropped_width = im.width / crop.width;
        let uncropped_height = im.height / crop.height;
        (
            im.x - crop.x * uncropped_width,
            im.y - crop.y * uncropped_height,
            uncropped_width,
            uncropped_height,
        )
    } else {
        (im.x, im.y, im.width, im.height)
    };

    // Flip the cropped pixels around the element box, leaving its clip fixed.
    let mut transform = String::new();
    if im.flip_x || im.flip_y {
        let sx = if im.flip_x { -1.0 } else { 1.0 };
        let sy = if im.flip_y { -1.0 } else { 1.0 };
        let tx = if im.flip_x { 2.0 * im.x + im.width } else { 0.0 };
        let ty = if im.flip_y { 2.0 * im.y + im.height } else { 0.0 };
        transform = format!(" transform=\"translate({tx} {ty}) scale({sx} {sy})\"");
    }
    let image = format!(
        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"none\"{}/>",
        x,
        y,
        width,
        height,
        escape_xml(&im.data_url),
        transform
    );
    if im.crop.is_none() && im.corner_radius == 0.0 {
        return image;
    }

    let id = format!("excaliosa-image-clip-{index}");
    let radius = if im.corner_radius > 0.0 {
        format!(" rx=\"{}\" ry=\"{}\"", im.corner_radius, im.corner_radius)
    } else {
        String::new()
    };
    format!(
        "<defs><clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{radius}/></clipPath></defs>\n<g clip-path=\"url(#{id})\">{image}</g>",
        im.x, im.y, im.width, im.height
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
    let sanitized: String = s
        .chars()
        .map(|ch| match ch {
            '\u{9}'
            | '\u{a}'
            | '\u{d}'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => ch,
            _ => '\u{fffd}',
        })
        .collect();
    sanitized
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
