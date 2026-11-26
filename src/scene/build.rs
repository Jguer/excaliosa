//! Lower an Excalidraw document into the backend-agnostic [`Scene`].
//!
//! All shape/rough/arrowhead/text geometry lives here so the SVG and Skia
//! backends stay thin and produce identical geometry. Rough geometry is
//! generated once via `roughr`; its `OpSet`s are converted into the neutral
//! [`PathOp`] representation both backends consume.

use euclid::default::Point2D;
use palette::Srgba;
use roughr::core::{Drawable as RoughDrawable, FillStyle, OpSet, OpSetType, OpType, Options};
use roughr::generator::Generator;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::arrow_utils::{calc_arrowhead_points, build_elbow_arrow_path, calculate_arrowhead_direction};
use crate::color_utils::{fill_color, has_stroke, parse_color};
use crate::font_utils::{get_font_family, text_metrics};
use crate::math_utils::calculate_center;
use crate::models::{ExcalidrawData, ExcalidrawElement};
use crate::rect_utils::{
    generate_rounded_diamond_path, generate_rounded_rect_path, get_corner_radius,
};
use crate::stroke_utils::get_stroke_dash_array;
use crate::utils::{calculate_viewbox, contributes_bounds};

use super::freedraw;
use super::static_elements::{build_embed, build_sticky_note};
use super::{
    Drawable, ImageCrop, ImagePrimitive, PaintedPath, PathOp, Primitive, Rgba, Scene, TextAlign,
    TextPrimitive,
};

/// Default font size used when an element omits one (Excalidraw "medium").
const DEFAULT_FONT_SIZE: f64 = 20.0;

/// Build a [`Scene`] from parsed Excalidraw data. `background` is stored for the
/// backend to fill before drawing.
pub fn build_scene(data: &ExcalidrawData, background: Option<Rgba>) -> Scene {
    let viewbox = calculate_viewbox(&data.elements);
    let generator = Generator::default();
    let mut unsupported = 0;

    let sticky_labels: HashMap<usize, usize> = data
        .elements
        .iter()
        .enumerate()
        .filter(|(_, el)| !el.is_deleted && el.element_type == "stickynote")
        .filter_map(|(index, _)| {
            sticky_bound_text(index, &data.elements).map(|text_index| (index, text_index))
        })
        .collect();
    let sticky_text_indices: HashSet<usize> = sticky_labels.values().copied().collect();
    let mut drawables = Vec::new();
    let mut embeds = Vec::new();

    for (index, el) in data
        .elements
        .iter()
        .enumerate()
        .filter(|(_, el)| !el.is_deleted)
    {
        unsupported += usize::from(!contributes_bounds(&el.element_type));
        if matches!(el.element_type.as_str(), "iframe" | "embeddable") {
            embeds.extend(build_embed(&generator, el));
        } else if el.element_type == "stickynote" {
            let text = sticky_labels
                .get(&index)
                .map(|text_index| &data.elements[*text_index]);
            drawables.extend(build_sticky_note(el, text));
            if let Some(text) = text
                && let Some(label) = build_element(&generator, text, &data.files, background)
            {
                drawables.push(label);
            }
        } else if !sticky_text_indices.contains(&index)
            && let Some(drawable) = build_element(&generator, el, &data.files, background)
        {
            drawables.push(drawable);
        }
    }
    // Iframe-like placeholders always paint above ordinary scene elements.
    drawables.extend(embeds);
    if unsupported > 0 {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "Skipped {unsupported} unsupported Excalidraw element(s)"
        );
    }

    Scene {
        viewbox,
        background,
        drawables,
    }
}

fn build_element(
    generator: &Generator,
    el: &ExcalidrawElement,
    files: &HashMap<String, Value>,
    background: Option<Rgba>,
) -> Option<Drawable> {
    let primitive = match el.element_type.as_str() {
        "rectangle" | "ellipse" | "diamond" => Primitive::Paths(build_shape(generator, el)),
        "line" | "arrow" => Primitive::Paths(build_line(generator, el, background)?),
        "freedraw" => Primitive::Paths(freedraw::build_freedraw(generator, el)?),
        "text" => Primitive::Text(build_text(el)?),
        "image" => Primitive::Image(build_image(el, files)?),
        _ => return None,
    };

    let mut drawable = drawable(el, primitive);
    drawable.link = el.link.clone();
    Some(drawable)
}

fn sticky_bound_text(note_index: usize, elements: &[ExcalidrawElement]) -> Option<usize> {
    let note = &elements[note_index];
    let mut bindings = note
        .bound_elements
        .as_ref()?
        .iter()
        .filter(|bound| bound.element_type == "text");
    let text_id = bindings.next()?.id.as_str();
    if bindings.next().is_some()
        || elements
            .iter()
            .filter(|el| !el.is_deleted && el.element_type == "stickynote" && el.id == note.id)
            .count()
            != 1
    {
        return None;
    }

    let mut matches = elements.iter().enumerate().filter(|(_, text)| {
        !text.is_deleted
            && text.element_type == "text"
            && text.id == text_id
            && text.container_id.as_deref() == Some(note.id.as_str())
    });
    let (text_index, _) = matches.next()?;
    matches.next().is_none().then_some(text_index)
}

pub(super) fn drawable(el: &ExcalidrawElement, primitive: Primitive) -> Drawable {
    let center = if el.element_type == "freedraw" {
        freedraw::point_center(el)
            .unwrap_or_else(|| calculate_center(el.x, el.y, el.width, el.height))
    } else {
        calculate_center(el.x, el.y, el.width, el.height)
    };
    // Excalidraw stores angles in radians; backends rotate in degrees.
    Drawable {
        primitive,
        link: None,
        opacity: (el.opacity / 100.0).clamp(0.0, 1.0),
        rotation_deg: el.angle.to_degrees(),
        center,
    }
}

// ---------------------------------------------------------------------------
// Closed shapes: rectangle, ellipse, diamond
// ---------------------------------------------------------------------------

pub(super) fn build_shape(generator: &Generator, el: &ExcalidrawElement) -> Vec<PaintedPath> {
    let should_stroke = has_stroke(el);
    let stroke = should_stroke.then(|| Rgba::from_tuple(parse_color(&el.stroke_color)));
    let fill = fill_color(el).map(Rgba::from_tuple);
    let dash = get_stroke_dash_array(&el.stroke_style, el.stroke_width);
    let options =
        |continuous_path| rough_options(el, should_stroke, fill.is_some(), continuous_path);
    let polygon = || {
        let points = vec![
            Point2D::new(el.x + el.width / 2.0, el.y),
            Point2D::new(el.x + el.width, el.y + el.height / 2.0),
            Point2D::new(el.x + el.width / 2.0, el.y + el.height),
            Point2D::new(el.x, el.y + el.height / 2.0),
        ];
        generator.polygon::<f64>(&points, &Some(options(false)))
    };

    let drawable = match el.element_type.as_str() {
        "rectangle" | "iframe" | "embeddable" => {
            let radius = if el.roundness.is_some() {
                get_corner_radius(el.width.min(el.height), el)
            } else {
                0.0
            };
            if radius > 0.0 {
                let d = generate_rounded_rect_path(el.x, el.y, el.width, el.height, radius);
                generator.path::<f64>(d, &Some(options(true)))
            } else {
                generator.rectangle::<f64>(el.x, el.y, el.width, el.height, &Some(options(false)))
            }
        }
        "ellipse" => {
            let (cx, cy) = calculate_center(el.x, el.y, el.width, el.height);
            generator.ellipse::<f64>(cx, cy, el.width, el.height, &Some(options(false)))
        }
        // diamond
        _ if el.roundness.is_some() => {
            if let Some(d) = generate_rounded_diamond_path(el) {
                generator.path::<f64>(d, &Some(options(true)))
            } else {
                polygon()
            }
        }
        _ => polygon(),
    };

    lower_drawable(&drawable, stroke, fill, el.stroke_width, dash)
}

// ---------------------------------------------------------------------------
// Lines and arrows
// ---------------------------------------------------------------------------

fn build_line(
    generator: &Generator,
    el: &ExcalidrawElement,
    background: Option<Rgba>,
) -> Option<Vec<PaintedPath>> {
    let points = el.points.as_ref()?;
    if points.is_empty() {
        return None;
    }

    let abs: Vec<(f64, f64)> = points.iter().map(|(px, py)| (el.x + px, el.y + py)).collect();
    let should_stroke = has_stroke(el);
    let stroke = should_stroke.then(|| Rgba::from_tuple(parse_color(&el.stroke_color)));
    let dash = get_stroke_dash_array(&el.stroke_style, el.stroke_width);
    let elbowed = el.element_type == "arrow" && el.elbowed.unwrap_or(false);
    let curved = !elbowed && el.roundness.is_some() && points.len() >= 3;
    // Upstream considers a line closed when its endpoints are within 8 px at 1× zoom.
    let first = points[0];
    let last = points[points.len() - 1];
    let fill = if el.element_type == "line"
        && points.len() >= 3
        && (last.0 - first.0).hypot(last.1 - first.1) <= 8.0
    {
        fill_color(el).map(Rgba::from_tuple)
    } else {
        None
    };
    let opts = rough_options(el, should_stroke, fill.is_some(), elbowed);
    let shaft = if elbowed && abs.len() >= 3 {
        match build_elbow_arrow_path(&abs, 16.0) {
            Some(d) => generator.path::<f64>(d, &Some(opts)),
            None => generator.linear_path::<f64>(&to_points(&abs), false, &Some(opts)),
        }
    } else if curved {
        generator.curve::<f64>(&to_points(&abs), &Some(opts))
    } else if fill.is_some() {
        generator.polygon::<f64>(&to_points(&abs), &Some(opts))
    } else {
        generator.linear_path::<f64>(&to_points(&abs), false, &Some(opts))
    };

    let mut out = lower_drawable(&shaft, stroke, fill, el.stroke_width, dash);

    // Arrowheads (arrows only), drawn cleanly (no roughness).
    if el.element_type == "arrow" {
        if let Some(s) = stroke {
            out.extend(arrowhead_paths(el, points, "start", s, background, curved));
            out.extend(arrowhead_paths(el, points, "end", s, background, curved));
        }
    }

    Some(out)
}

fn arrowhead_paths(
    el: &ExcalidrawElement,
    points: &[(f64, f64)],
    position: &str,
    stroke: Rgba,
    background: Option<Rgba>,
    curved: bool,
) -> Vec<PaintedPath> {
    let kind = match position {
        "start" => el.start_arrowhead.as_deref().or(el.start_arrow_type.as_deref()),
        _ => el.end_arrowhead.as_deref().or(el.end_arrow_type.as_deref()),
    };
    let Some(kind) = kind else {
        return vec![];
    };
    if points.len() < 2 {
        return vec![];
    }

    let direction = if curved {
        calculate_arrowhead_direction(points, el.x, el.y, position, 0.5)
            .unwrap_or_else(|| straight_direction(points, el.x, el.y, position))
    } else {
        straight_direction(points, el.x, el.y, position)
    };

    let (tail_x, tail_y, tip_x, tip_y, seg_len) = direction;
    let make = |kind| {
        let vals = calc_arrowhead_points(tail_x, tail_y, tip_x, tip_y, kind, el.stroke_width, seg_len);
        arrowhead_shape(kind, &vals, stroke, el.stroke_width, background)
    };
    if kind == "crowfoot_one_or_many" {
        let mut paths = make("crowfoot_many");
        paths.extend(make("crowfoot_one"));
        paths
    } else {
        make(kind)
    }
}

/// Fallback tip/tail from the raw endpoint segment when curve tangent is unavailable.
fn straight_direction(
    points: &[(f64, f64)],
    x: f64,
    y: f64,
    position: &str,
) -> (f64, f64, f64, f64, f64) {
    let (tip_rel, tail_rel) = if position == "start" {
        (points[0], points[1])
    } else {
        (points[points.len() - 1], points[points.len() - 2])
    };
    let tip = (x + tip_rel.0, y + tip_rel.1);
    let tail = (x + tail_rel.0, y + tail_rel.1);
    let seg_len = ((tip.0 - tail.0).powi(2) + (tip.1 - tail.1).powi(2)).sqrt();
    (tail.0, tail.1, tip.0, tip.1, seg_len)
}

fn arrowhead_shape(
    kind: &str,
    vals: &[f64],
    stroke: Rgba,
    sw: f64,
    background: Option<Rgba>,
) -> Vec<PaintedPath> {
    // shortcut: transparent outlines don't mask the shaft; clip it if knockout is needed.
    let fill = if kind.ends_with("_outline") {
        background.filter(|color| color.3 != 0)
    } else {
        Some(stroke)
    };
    let outlined = |ops: Vec<PathOp>| PaintedPath {
        ops,
        fill,
        stroke: Some(stroke),
        stroke_width: sw,
        dash: None,
        fill_even_odd: false,
    };

    match kind {
        "dot" | "circle" | "circle_outline" if vals.len() >= 3 => {
            vec![outlined(circle_ops(vals[0], vals[1], vals[2] / 2.0))]
        }
        "bar" if vals.len() >= 4 => vec![PaintedPath::stroke_only(
            vec![PathOp::Move(vals[0], vals[1]), PathOp::Line(vals[2], vals[3])],
            stroke,
            sw,
        )],
        "crowfoot_one" if vals.len() >= 6 => vec![PaintedPath::stroke_only(
            vec![PathOp::Move(vals[2], vals[3]), PathOp::Line(vals[4], vals[5])],
            stroke,
            sw,
        )],
        "arrow" | "crowfoot_many" if vals.len() >= 6 => vec![PaintedPath::stroke_only(
            vec![
                PathOp::Move(vals[2], vals[3]),
                PathOp::Line(vals[0], vals[1]),
                PathOp::Move(vals[4], vals[5]),
                PathOp::Line(vals[0], vals[1]),
            ],
            stroke,
            sw,
        )],
        "triangle" | "triangle_outline" if vals.len() >= 6 => vec![outlined(vec![
            PathOp::Move(vals[0], vals[1]),
            PathOp::Line(vals[2], vals[3]),
            PathOp::Line(vals[4], vals[5]),
            PathOp::Close,
        ])],
        "diamond" | "diamond_outline" if vals.len() >= 8 => vec![outlined(vec![
            PathOp::Move(vals[0], vals[1]),
            PathOp::Line(vals[2], vals[3]),
            PathOp::Line(vals[4], vals[5]),
            PathOp::Line(vals[6], vals[7]),
            PathOp::Close,
        ])],
        _ => vec![],
    }
}

/// Cubic-bezier approximation of a circle (kappa constant).
fn circle_ops(cx: f64, cy: f64, r: f64) -> Vec<PathOp> {
    let k = 0.552_284_749_830_793_6 * r;
    vec![
        PathOp::Move(cx + r, cy),
        PathOp::Cubic(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
        PathOp::Cubic(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
        PathOp::Cubic(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
        PathOp::Cubic(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        PathOp::Close,
    ]
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

fn build_text(el: &ExcalidrawElement) -> Option<TextPrimitive> {
    let content = el.text.clone().unwrap_or_default();
    if content.is_empty() {
        return None;
    }
    let font_size = el.font_size.unwrap_or(DEFAULT_FONT_SIZE);
    let font_family = get_font_family(el.font_family);
    let (line_height, baseline_offset) = text_metrics(font_family, font_size, el.line_height);
    let align = match el.text_align.as_deref() {
        Some("center") => TextAlign::Center,
        Some("right") => TextAlign::Right,
        _ => TextAlign::Left,
    };
    Some(TextPrimitive {
        content,
        x: el.x,
        y: el.y,
        // The stored y already incorporates container vertical alignment.
        baseline_offset,
        font_size,
        font_family,
        color: Rgba::from_tuple(parse_color(&el.stroke_color)),
        align,
        line_height,
        container_width: el.width,
    })
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

fn build_image(el: &ExcalidrawElement, files: &HashMap<String, Value>) -> Option<ImagePrimitive> {
    let file_id = el.file_id.as_deref()?;
    let data_url = files.get(file_id)?.get("dataURL")?.as_str()?.to_string();
    if !is_allowed_raster_data_url(&data_url) {
        return None;
    }
    // Negative scale components flip the image on that axis.
    let flip_x = el.scale.as_ref().and_then(|s| s.first()).is_some_and(|v| *v < 0.0);
    let flip_y = el.scale.as_ref().and_then(|s| s.get(1)).is_some_and(|v| *v < 0.0);
    let crop = validated_image_crop(el);
    let corner_radius = if el.roundness.is_some() {
        let radius = get_corner_radius(el.width.min(el.height), el);
        if radius.is_finite() {
            radius
                .max(0.0)
                .min((el.width.min(el.height) / 2.0).max(0.0))
        } else {
            0.0
        }
    } else {
        0.0
    };
    Some(ImagePrimitive {
        x: el.x,
        y: el.y,
        width: el.width,
        height: el.height,
        data_url,
        flip_x,
        flip_y,
        crop,
        corner_radius,
    })
}

fn validated_image_crop(el: &ExcalidrawElement) -> Option<ImageCrop> {
    let value = el.crop.as_ref()?;
    let field = |name: &str| value.get(name)?.as_f64().filter(|v| v.is_finite());
    let (x, y, width, height) = (field("x")?, field("y")?, field("width")?, field("height")?);
    let natural_width = field("naturalWidth")?;
    let natural_height = field("naturalHeight")?;
    // Excalidraw's derived source-edge values can exceed the natural size by a few ulps.
    let x_margin = natural_width * f64::EPSILON * 8.0;
    let y_margin = natural_height * f64::EPSILON * 8.0;
    if x < -x_margin
        || y < -y_margin
        || width <= 0.0
        || height <= 0.0
        || natural_width <= 0.0
        || natural_height <= 0.0
        || x + width > natural_width + x_margin
        || y + height > natural_height + y_margin
    {
        return None;
    }
    let x = x.clamp(0.0, natural_width);
    let y = y.clamp(0.0, natural_height);
    let width = width.min(natural_width - x);
    let height = height.min(natural_height - y);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let crop = ImageCrop {
        x: x / natural_width,
        y: y / natural_height,
        width: width / natural_width,
        height: height / natural_height,
    };
    // The same ratios drive both backends' uncropped placement and sampling.
    let uncropped_width = el.width / crop.width;
    let uncropped_height = el.height / crop.height;
    if !uncropped_width.is_finite() || !uncropped_height.is_finite() {
        return None;
    }

    // PNG performs these operations in f32; reject values it cannot represent safely.
    let raster_crop_width = crop.width as f32;
    let raster_crop_height = crop.height as f32;
    let raster_offset_x = (crop.x * uncropped_width) as f32;
    let raster_offset_y = (crop.y * uncropped_height) as f32;
    if raster_crop_width <= 0.0
        || raster_crop_height <= 0.0
        || ![
            el.width as f32 / raster_crop_width,
            el.height as f32 / raster_crop_height,
            raster_offset_x,
            raster_offset_y,
            raster_offset_x + el.width as f32,
            raster_offset_y + el.height as f32,
        ]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }
    Some(crop)
}

/// Allowlist raster image data URLs. SVG data URLs are rejected (they can carry
/// scripts/event handlers).
fn is_allowed_raster_data_url(data_url: &str) -> bool {
    let lower = data_url.trim().to_ascii_lowercase();
    lower.starts_with("data:image/png;")
        || lower.starts_with("data:image/jpeg;")
        || lower.starts_with("data:image/jpg;")
        || lower.starts_with("data:image/gif;")
        || lower.starts_with("data:image/webp;")
}

// ---------------------------------------------------------------------------
// roughr helpers
// ---------------------------------------------------------------------------

/// Build roughr options for an element. Colors only signal *presence* of a
/// stroke/fill (the real colors are applied when lowering the op-sets), so
/// geometry generation matches regardless of the element's actual palette.
pub(super) fn rough_options(
    el: &ExcalidrawElement,
    should_stroke: bool,
    should_fill: bool,
    continuous_path: bool,
) -> Options {
    let present = Srgba::new(0.0, 0.0, 0.0, 1.0);
    let mut o = Options::default();
    o.roughness = Some(adjusted_roughness(el) as f32);
    o.preserve_vertices = Some(continuous_path || el.roughness < 2.0);
    if el.element_type == "ellipse" {
        o.curve_fitting = Some(1.0);
    }
    o.seed = Some(el.seed as u64);
    let non_solid = el.stroke_style != "solid";
    let width = el.stroke_width as f32;
    o.disable_multi_stroke = Some(non_solid);
    o.stroke_width = Some(if non_solid { width + 0.5 } else { width });
    o.fill_weight = Some(width / 2.0);
    o.hachure_gap = Some(width * 4.0);
    o.stroke = should_stroke.then_some(present);
    o.fill = should_fill.then_some(present);
    o.fill_style = Some(match el.fill_style.as_str() {
        "solid" => FillStyle::Solid,
        "cross-hatch" => FillStyle::CrossHatch,
        "zigzag" => FillStyle::ZigZag,
        _ => FillStyle::Hachure,
    });
    o
}

fn adjusted_roughness(el: &ExcalidrawElement) -> f64 {
    let roughness = el.roughness.max(0.0);
    let max_size = el.width.max(el.height);
    let min_size = el.width.min(el.height);
    let roundable = matches!(
        el.element_type.as_str(),
        "rectangle" | "iframe" | "embeddable" | "line" | "diamond" | "stickynote" | "image"
    );
    let linear = matches!(el.element_type.as_str(), "line" | "arrow");

    if (min_size >= 20.0 && max_size >= 50.0)
        || (min_size >= 15.0 && el.roundness.is_some() && roundable)
        || (linear && max_size >= 50.0)
    {
        roughness
    } else {
        (roughness / if max_size < 10.0 { 3.0 } else { 2.0 }).min(2.5)
    }
}

/// Convert a roughr [`Drawable`] into painted paths, mapping each op-set to the
/// element's real colors using the same rules as `roughr`'s `to_paths`.
pub(super) fn lower_drawable(
    drawable: &RoughDrawable<f64>,
    stroke: Option<Rgba>,
    fill: Option<Rgba>,
    stroke_width: f64,
    dash: Option<Vec<f64>>,
) -> Vec<PaintedPath> {
    // Fill-sketch line weight: roughr uses stroke_width/2 when fill_weight < 0.
    let fill_weight = match drawable.options.fill_weight {
        Some(w) if w >= 0.0 => w as f64,
        _ => (stroke_width / 2.0).max(0.5),
    };
    let stroke_width = drawable.options.stroke_width.map_or(stroke_width, f64::from);
    let even_odd = matches!(drawable.shape.as_str(), "curve" | "polygon" | "path");

    let mut out = Vec::new();
    for set in &drawable.sets {
        let ops = convert_ops(set);
        if ops.is_empty() {
            continue;
        }
        match set.op_set_type {
            OpSetType::Path => {
                if let Some(s) = stroke {
                    out.push(PaintedPath {
                        ops,
                        fill: None,
                        stroke: Some(s),
                        stroke_width,
                        dash: dash.clone(),
                        fill_even_odd: false,
                    });
                }
            }
            OpSetType::FillPath => {
                if let Some(f) = fill {
                    out.push(PaintedPath {
                        ops,
                        fill: Some(f),
                        stroke: None,
                        stroke_width: 0.0,
                        dash: None,
                        fill_even_odd: even_odd,
                    });
                }
            }
            OpSetType::FillSketch => {
                if let Some(f) = fill {
                    out.push(PaintedPath::stroke_only(ops, f, fill_weight));
                }
            }
        }
    }
    out
}

fn convert_ops(set: &OpSet<f64>) -> Vec<PathOp> {
    set.ops
        .iter()
        .filter_map(|op| match op.op {
            OpType::Move => Some(PathOp::Move(op.data[0], op.data[1])),
            OpType::LineTo => Some(PathOp::Line(op.data[0], op.data[1])),
            OpType::BCurveTo => Some(PathOp::Cubic(
                op.data[0], op.data[1], op.data[2], op.data[3], op.data[4], op.data[5],
            )),
        })
        .collect()
}

pub(super) fn to_points(points: &[(f64, f64)]) -> Vec<Point2D<f64>> {
    points.iter().map(|(x, y)| Point2D::new(*x, *y)).collect()
}

#[cfg(test)]
mod rough_option_tests {
    use super::*;
    use crate::models::RoundnessType;

    fn element(kind: &str, width: f64, height: f64, rounded: bool) -> ExcalidrawElement {
        ExcalidrawElement {
            element_type: kind.into(),
            width,
            height,
            roughness: 4.0,
            roundness: rounded.then_some(RoundnessType {
                roundness_type: 2,
                value: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn roughness_reduction_and_exemption_boundaries() {
        for (kind, width, height, rounded, expected) in [
            ("rectangle", 9.0, 9.0, false, 4.0 / 3.0),
            ("rectangle", 10.0, 9.0, false, 2.0),
            ("rectangle", 20.0, 50.0, false, 4.0),
            ("rectangle", 19.0, 50.0, false, 2.0),
            ("rectangle", 15.0, 15.0, true, 4.0),
            ("ellipse", 15.0, 15.0, true, 2.0),
            ("line", 50.0, 0.0, false, 4.0),
            ("arrow", 15.0, 15.0, true, 2.0),
            ("rectangle", 50.0, 0.0, false, 2.0),
        ] {
            let el = element(kind, width, height, rounded);
            assert_eq!(adjusted_roughness(&el), expected, "{kind} {width}x{height}");
        }

        let mut capped = element("rectangle", 10.0, 10.0, false);
        capped.roughness = 9.0;
        assert_eq!(adjusted_roughness(&capped), 2.5);
    }

    #[test]
    fn vertices_and_ellipse_curve_fitting_follow_upstream_options() {
        let el = element("rectangle", 100.0, 100.0, false);
        assert_eq!(
            rough_options(&el, true, false, false).preserve_vertices,
            Some(false)
        );
        assert_eq!(
            rough_options(&el, true, false, true).preserve_vertices,
            Some(true)
        );

        let mut default_roughness = el.clone();
        default_roughness.roughness = 1.0;
        assert_eq!(
            rough_options(&default_roughness, true, false, false).preserve_vertices,
            Some(true)
        );

        let mut cartoonist_roughness = el.clone();
        cartoonist_roughness.roughness = 2.0;
        assert_eq!(
            rough_options(&cartoonist_roughness, true, false, false).preserve_vertices,
            Some(false)
        );

        let ellipse = element("ellipse", 100.0, 100.0, false);
        let ellipse_options = rough_options(&ellipse, true, false, false);
        assert_eq!(ellipse_options.curve_fitting, Some(1.0));
        assert_eq!(ellipse_options.preserve_vertices, Some(false));
        assert_eq!(
            rough_options(&el, true, false, false).curve_fitting,
            Some(0.95)
        );
    }
}
