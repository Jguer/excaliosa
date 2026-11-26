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
use roughr::renderer::svg_path;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::arrow_utils::{
    CARDINALITY_INNER_BAR_OFFSET, CARDINALITY_ONE_OR_MANY_BAR_OFFSET,
    CARDINALITY_ZERO_CIRCLE_OFFSET, CARDINALITY_ZERO_CIRCLE_SCALE, build_elbow_arrow_path,
    calc_arrowhead_points_at_size, normalize_arrowhead, resolve_arrowhead_size,
};
use crate::color_utils::{fill_color, has_stroke, parse_color, parse_scene_color};
use crate::font_utils::{get_font_family, text_metrics};
use crate::math_utils::calculate_center;
use crate::models::{
    ArrowheadField, ExcalidrawData, ExcalidrawElement, ViewBox, has_renderable_elbow_geometry,
};
use crate::rect_utils::{
    generate_rounded_diamond_path, generate_rounded_rect_path, get_corner_radius,
};
use crate::stroke_utils::{get_dotted_cap_dash_array, get_stroke_dash_array};
use crate::utils::{calculate_viewbox_with_labels, contributes_bounds};

use super::arrow_label::{ResolvedArrowLabel, resolve_arrow_labels};
use super::frame::{build_frame, containing_frame, frame_clip};
use super::freedraw;
use super::linear_geometry::{LinearMeasurements, generated_cap_direction, measure_linear};
use super::static_elements::{build_embed, build_sticky_note};
use super::{
    Drawable, ImageAsset, ImageCrop, ImagePrimitive, PaintedPath, PathOp, Primitive, Rgba, Scene,
    TextAlign, TextPrimitive,
};

/// Default font size used when an element omits one (Excalidraw "medium").
const DEFAULT_FONT_SIZE: f64 = 20.0;

/// Build a [`Scene`] from parsed Excalidraw data. `background` is stored for the
/// backend to fill before drawing.
pub fn build_scene(data: &ExcalidrawData, background: Option<Rgba>) -> Scene {
    let linear_measurements = measure_linear_elements(&data.elements);
    let arrow_labels = resolve_arrow_labels(&data.elements, &linear_measurements);
    let viewbox =
        calculate_viewbox_with_labels(&data.elements, &arrow_labels, &linear_measurements);
    build_scene_with_labels(
        data,
        background,
        viewbox,
        arrow_labels,
        &linear_measurements,
    )
}

pub(crate) fn build_scene_with_labels(
    data: &ExcalidrawData,
    background: Option<Rgba>,
    viewbox: ViewBox,
    arrow_labels: HashMap<usize, ResolvedArrowLabel>,
    linear_measurements: &LinearMeasurements,
) -> Scene {
    let cap_background = background
        .or_else(|| {
            data.app_state
                .get("viewBackgroundColor")
                .and_then(Value::as_str)
                .and_then(|color| parse_scene_color(color).ok())
                .map(Rgba::from_tuple)
        })
        .unwrap_or(Rgba(255, 255, 255, 255));
    let generator = Generator::default();
    let elements: HashMap<&str, &ExcalidrawElement> = data
        .elements
        .iter()
        .map(|el| (el.id.as_str(), el))
        .collect();
    let mut unsupported = 0;

    let bound_text: HashSet<usize> = arrow_labels
        .values()
        .map(|label| label.text_index)
        .collect();
    let shape_labels = resolve_shape_labels(&data.elements, &bound_text);
    let shape_text_indices: HashSet<usize> = shape_labels.values().copied().collect();
    let mut drawables = Vec::new();
    let mut embeds = Vec::new();
    let mut image_assets = ImageAssets::default();

    for (index, el) in data
        .elements
        .iter()
        .enumerate()
        .filter(|(_, el)| !el.is_deleted)
    {
        unsupported += usize::from(!contributes_bounds(&el.element_type));
        if matches!(el.element_type.as_str(), "frame" | "magicframe") {
            drawables.extend(build_frame(el));
        } else if matches!(el.element_type.as_str(), "iframe" | "embeddable") {
            let mut built = build_embed(&generator, el);
            apply_containing_frame(&mut built, el, &elements);
            embeds.extend(built);
        } else if el.element_type == "stickynote" {
            let text = shape_labels
                .get(&index)
                .map(|text_index| &data.elements[*text_index]);
            let mut built = build_sticky_note(el, text);
            apply_containing_frame(&mut built, el, &elements);
            drawables.extend(built);
            if let Some(text) = text
                && let Some(mut label) = build_element(
                    &generator,
                    text,
                    &data.files,
                    cap_background,
                    &mut image_assets,
                )
            {
                apply_containing_frame(std::slice::from_mut(&mut label), text, &elements);
                drawables.push(label);
            }
        } else if !shape_text_indices.contains(&index)
            && !bound_text.contains(&index)
            && let Some(mut drawable) = build_element(
                &generator,
                el,
                &data.files,
                cap_background,
                &mut image_assets,
            )
        {
            if let Some(measurement) = linear_measurements.get(&index) {
                drawable.center = measurement.center;
            }
            if let Some(label) = arrow_labels.get(&index) {
                drawable.label_gap = Some(label.gap);
                let label_drawable = build_element(
                    &generator,
                    &data.elements[label.text_index],
                    &data.files,
                    cap_background,
                    &mut image_assets,
                )
                .map(|mut label_drawable| {
                    if let Primitive::Text(text) = &mut label_drawable.primitive {
                        text.x = label.x;
                        text.y = label.y;
                    }
                    let element = &data.elements[label.text_index];
                    label_drawable.center =
                        calculate_center(label.x, label.y, element.width, element.height);
                    label_drawable
                });
                apply_containing_frame(std::slice::from_mut(&mut drawable), el, &elements);
                drawables.push(drawable);
                if let Some(mut label_drawable) = label_drawable {
                    let element = &data.elements[label.text_index];
                    apply_containing_frame(
                        std::slice::from_mut(&mut label_drawable),
                        element,
                        &elements,
                    );
                    drawables.push(label_drawable);
                }
            } else {
                apply_containing_frame(std::slice::from_mut(&mut drawable), el, &elements);
                drawables.push(drawable);
            }
            if let Some(text_index) = shape_labels.get(&index)
                && let Some(mut label) = build_element(
                    &generator,
                    &data.elements[*text_index],
                    &data.files,
                    cap_background,
                    &mut image_assets,
                )
            {
                let text = &data.elements[*text_index];
                apply_containing_frame(std::slice::from_mut(&mut label), text, &elements);
                drawables.push(label);
            }
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

fn apply_containing_frame(
    drawables: &mut [Drawable],
    element: &ExcalidrawElement,
    elements: &HashMap<&str, &ExcalidrawElement>,
) {
    let Some(frame) = containing_frame(element, elements) else {
        return;
    };
    let frame_opacity = (frame.opacity / 100.0).clamp(0.0, 1.0);
    let clip = frame_clip(element, frame, elements);
    for drawable in drawables {
        drawable.opacity *= frame_opacity;
        drawable.frame_opacity = Some(frame_opacity);
        drawable.frame_clip = clip.clone();
    }
}

fn build_element(
    generator: &Generator,
    el: &ExcalidrawElement,
    files: &HashMap<String, Value>,
    cap_background: Rgba,
    image_assets: &mut ImageAssets,
) -> Option<Drawable> {
    let primitive = match el.element_type.as_str() {
        "rectangle" | "ellipse" | "diamond" => Primitive::Paths(build_shape(generator, el)),
        "line" | "arrow" => Primitive::Paths(build_line(generator, el, cap_background)?),
        "freedraw" => Primitive::Paths(freedraw::build_freedraw(generator, el)?),
        "text" => Primitive::Text(build_text(el)?),
        "image" => Primitive::Image(build_image(el, files, image_assets)?),
        _ => return None,
    };

    let mut drawable = drawable(el, primitive);
    drawable.link = el.link.clone();
    Some(drawable)
}

fn resolve_shape_labels(
    elements: &[ExcalidrawElement],
    arrow_text: &HashSet<usize>,
) -> HashMap<usize, usize> {
    let mut live_ids = HashMap::new();
    let mut text_claims: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, element) in elements.iter().enumerate() {
        if element.is_deleted {
            continue;
        }
        if !element.id.is_empty() {
            live_ids
                .entry(element.id.as_str())
                .and_modify(|found| *found = None)
                .or_insert(Some(index));
        }
        if matches!(
            element.element_type.as_str(),
            "rectangle" | "ellipse" | "diamond" | "stickynote" | "arrow"
        ) {
            for bound in element
                .bound_elements
                .iter()
                .flatten()
                .filter(|bound| bound.element_type == "text")
            {
                text_claims.entry(&bound.id).or_default().push(index);
            }
        }
    }

    let mut labels = HashMap::new();
    for (owner_index, owner) in elements.iter().enumerate() {
        if owner.is_deleted
            || !matches!(
                owner.element_type.as_str(),
                "rectangle" | "ellipse" | "diamond" | "stickynote"
            )
            || live_ids.get(owner.id.as_str()) != Some(&Some(owner_index))
        {
            continue;
        }
        let mut bindings = owner
            .bound_elements
            .iter()
            .flatten()
            .filter(|bound| bound.element_type == "text");
        let Some(binding) = bindings.next() else {
            continue;
        };
        if bindings.next().is_some()
            || text_claims
                .get(binding.id.as_str())
                .is_none_or(|claims| claims.as_slice() != [owner_index])
        {
            continue;
        }
        let Some(text_index) = live_ids.get(binding.id.as_str()).and_then(|index| *index) else {
            continue;
        };
        let text = &elements[text_index];
        if !arrow_text.contains(&text_index)
            && text.element_type == "text"
            && text.container_id.as_deref() == Some(owner.id.as_str())
        {
            labels.insert(owner_index, text_index);
        }
    }
    labels
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
        label_gap: None,
        opacity: (el.opacity / 100.0).clamp(0.0, 1.0),
        rotation_deg: el.angle.to_degrees(),
        center,
        frame_clip: None,
        frame_opacity: None,
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

pub(crate) fn measure_linear_elements(elements: &[ExcalidrawElement]) -> LinearMeasurements {
    let generator = Generator::default();
    elements
        .iter()
        .enumerate()
        .filter(|(_, el)| !el.is_deleted && matches!(el.element_type.as_str(), "line" | "arrow"))
        .map(|(index, el)| {
            let outline = generate_linear_shaft(&generator, el, true).map(|(shaft, _)| {
                lower_drawable(
                    &shaft,
                    Some(Rgba(0, 0, 0, 255)),
                    None,
                    el.stroke_width,
                    None,
                )
            });
            (index, measure_linear(el, outline.as_deref()))
        })
        .collect()
}

fn build_line(
    generator: &Generator,
    el: &ExcalidrawElement,
    cap_background: Rgba,
) -> Option<Vec<PaintedPath>> {
    let points = el.points.as_ref()?;
    let (shaft, fill) = generate_linear_shaft(generator, el, false)?;
    let should_stroke = has_stroke(el);
    let stroke = should_stroke.then(|| Rgba::from_tuple(parse_color(&el.stroke_color)));
    let dash = get_stroke_dash_array(&el.stroke_style, el.stroke_width);
    let cap_ops = shaft
        .sets
        .iter()
        .find(|set| set.op_set_type == OpSetType::Path)
        .map(convert_ops);
    let mut out = lower_drawable(&shaft, stroke, fill, el.stroke_width, dash);

    // Caps share the element seed but apply their own roughness and dash rules.
    if el.element_type == "arrow" {
        if let Some(s) = stroke.filter(|stroke| stroke.3 > 0) {
            out.extend(arrowhead_paths(
                generator,
                el,
                points,
                cap_ops.as_deref(),
                "start",
                s,
                cap_background,
            ));
            out.extend(arrowhead_paths(
                generator,
                el,
                points,
                cap_ops.as_deref(),
                "end",
                s,
                cap_background,
            ));
        }
    }

    Some(out)
}

/// Select and generate the shaft once for painting or outline-only preflight.
fn generate_linear_shaft(
    generator: &Generator,
    el: &ExcalidrawElement,
    outline_only: bool,
) -> Option<(RoughDrawable<f64>, Option<Rgba>)> {
    let points = el.points.as_ref()?;
    if points.is_empty() {
        return None;
    }
    let elbowed = el.element_type == "arrow" && el.elbowed.unwrap_or(false);
    if elbowed && !has_renderable_elbow_geometry(el) {
        return None;
    }

    let abs: Vec<(f64, f64)> = points
        .iter()
        .map(|(px, py)| (el.x + px, el.y + py))
        .collect();
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
    let mut opts = rough_options(
        el,
        outline_only || has_stroke(el),
        !outline_only && fill.is_some(),
        elbowed,
    );
    if !outline_only && fill.is_some() && el.fill_style != "solid" {
        let gap = f64::from(opts.hachure_gap.unwrap_or(0.0).max(0.1));
        // roughr scans rotated world-space polygons; keep the outline when its step rounds away.
        let scan_advances = abs.iter().all(|(x, y)| {
            let extent = x.abs() + y.abs();
            extent.is_finite() && extent + gap > extent && -extent + gap > -extent
        });
        if !scan_advances {
            opts.fill = None;
        }
    }
    let shaft = if elbowed && abs.len() >= 3 {
        match build_elbow_arrow_path(&abs, 16.0) {
            Some(d) if opts.fill.is_none() => {
                let mut options = opts;
                let sets = if options.stroke.is_some() {
                    vec![svg_path(d, &mut options)]
                } else {
                    vec![]
                };
                RoughDrawable {
                    shape: "path".into(),
                    options,
                    sets,
                }
            }
            Some(d) => generator.path::<f64>(d, &Some(opts)),
            None => generator.linear_path::<f64>(&to_points(&abs), false, &Some(opts)),
        }
    } else if curved {
        generator.curve::<f64>(&to_points(&abs), &Some(opts))
    } else if fill.is_some() {
        // Keep polygon selection during preflight, but omit its fill generation.
        generator.polygon::<f64>(&to_points(&abs), &Some(opts))
    } else {
        generator.linear_path::<f64>(&to_points(&abs), false, &Some(opts))
    };
    Some((shaft, fill))
}

fn arrowhead_paths(
    generator: &Generator,
    el: &ExcalidrawElement,
    points: &[(f64, f64)],
    shaft_ops: Option<&[PathOp]>,
    position: &str,
    stroke: Rgba,
    cap_background: Rgba,
) -> Vec<PaintedPath> {
    if points.len() < 2 {
        return vec![];
    }
    let (field, legacy, default) = if position == "start" {
        (&el.start_arrowhead, el.start_arrow_type.as_deref(), None)
    } else {
        (
            &el.end_arrowhead,
            el.end_arrow_type.as_deref(),
            Some("arrow"),
        )
    };
    let kind = match field {
        ArrowheadField::Named(kind) => Some(kind.as_str()),
        ArrowheadField::Null => None,
        ArrowheadField::Omitted => legacy.or(default),
    };
    let Some(kind) = kind.map(normalize_arrowhead) else {
        return vec![];
    };

    let Some((tail_x, tail_y, tip_x, tip_y)) = shaft_ops
        .and_then(|ops| generated_cap_direction(ops, position))
        .or_else(|| straight_direction(points, el.x, el.y, position))
    else {
        return vec![];
    };
    let elbowed = el.elbowed.unwrap_or(false);
    let curved = !elbowed && el.roundness.is_some() && points.len() >= 3;
    let make = |part: &str, offset: f64, diameter_scale: f64| {
        let size = resolve_arrowhead_size(
            points,
            position,
            part,
            kind,
            el.stroke_width,
            curved,
            elbowed,
        );
        if size == 0.0 {
            return vec![];
        }
        let vals = calc_arrowhead_points_at_size(
            tail_x,
            tail_y,
            tip_x,
            tip_y,
            part,
            el.stroke_width,
            size,
            offset,
        );
        arrowhead_shape(
            generator,
            el,
            part,
            &vals,
            stroke,
            cap_background,
            diameter_scale,
        )
    };
    match kind {
        "cardinality_one_or_many" => {
            let mut paths = make("cardinality_many", 0.0, 1.0);
            paths.extend(make(
                "cardinality_one",
                CARDINALITY_ONE_OR_MANY_BAR_OFFSET,
                1.0,
            ));
            paths
        }
        "cardinality_exactly_one" => {
            let mut paths = make("cardinality_one", CARDINALITY_INNER_BAR_OFFSET, 1.0);
            paths.extend(make("cardinality_one", 0.0, 1.0));
            paths
        }
        "cardinality_zero_or_one" => {
            let mut paths = make(
                "circle_outline",
                CARDINALITY_ZERO_CIRCLE_OFFSET,
                CARDINALITY_ZERO_CIRCLE_SCALE,
            );
            paths.extend(make("cardinality_one", CARDINALITY_INNER_BAR_OFFSET, 1.0));
            paths
        }
        "cardinality_zero_or_many" => {
            let mut paths = make("cardinality_many", 0.0, 1.0);
            paths.extend(make(
                "circle_outline",
                CARDINALITY_ZERO_CIRCLE_OFFSET,
                CARDINALITY_ZERO_CIRCLE_SCALE,
            ));
            paths
        }
        _ => make(kind, 0.0, 1.0),
    }
}

/// Fallback tip/tail from the raw endpoint segment when shaft geometry is unavailable.
fn straight_direction(
    points: &[(f64, f64)],
    x: f64,
    y: f64,
    position: &str,
) -> Option<(f64, f64, f64, f64)> {
    let (tip_rel, tail_rel) = if position == "start" {
        (points[0], points[1])
    } else {
        (points[points.len() - 1], points[points.len() - 2])
    };
    let tip = (x + tip_rel.0, y + tip_rel.1);
    let tail = (x + tail_rel.0, y + tail_rel.1);
    let length = (tip.0 - tail.0).hypot(tip.1 - tail.1);
    (length.is_finite() && length > 0.0).then_some((tail.0, tail.1, tip.0, tip.1))
}

fn arrowhead_shape(
    generator: &Generator,
    el: &ExcalidrawElement,
    kind: &str,
    vals: &[f64],
    stroke: Rgba,
    cap_background: Rgba,
    diameter_scale: f64,
) -> Vec<PaintedPath> {
    let mut options = rough_options(el, true, false, false);
    let closed = matches!(
        kind,
        "circle"
            | "circle_outline"
            | "triangle"
            | "triangle_outline"
            | "diamond"
            | "diamond_outline"
    );
    options.roughness = Some(el.roughness.max(0.0).min(
        if matches!(kind, "circle" | "circle_outline") {
            0.5
        } else {
            1.0
        },
    ) as f32);
    if closed {
        options.disable_multi_stroke = Some(true);
        options.preserve_vertices = Some(true);
    }
    let dash = (!closed && el.stroke_style == "dotted")
        .then(|| get_dotted_cap_dash_array(el.stroke_width));
    let line = |x1: f64, y1: f64, x2: f64, y2: f64| {
        lower_drawable(
            &generator.line::<f64>(x1, y1, x2, y2, &Some(options.clone())),
            Some(stroke),
            None,
            el.stroke_width,
            dash.clone(),
        )
    };
    let lines_to_tip = || {
        let mut paths = line(vals[2], vals[3], vals[0], vals[1]);
        paths.extend(line(vals[4], vals[5], vals[0], vals[1]));
        paths
    };
    let closed_fill = if kind.ends_with("_outline") {
        cap_background
    } else {
        stroke
    };

    match kind {
        "circle" | "circle_outline" if vals.len() >= 3 => {
            let diameter = vals[2] * diameter_scale;
            if diameter <= 0.0 {
                return vec![];
            }
            let drawable = generator.circle::<f64>(vals[0], vals[1], diameter, &Some(options));
            lower_filled_cap(&drawable, stroke, closed_fill, el.stroke_width)
        }
        "bar" | "cardinality_one" if vals.len() >= 6 => line(vals[2], vals[3], vals[4], vals[5]),
        "arrow" | "cardinality_many" if vals.len() >= 6 => lines_to_tip(),
        "triangle" | "triangle_outline" if vals.len() >= 6 => {
            let points = vec![
                Point2D::new(vals[0], vals[1]),
                Point2D::new(vals[2], vals[3]),
                Point2D::new(vals[4], vals[5]),
            ];
            let drawable = generator.polygon::<f64>(&points, &Some(options));
            lower_filled_cap(&drawable, stroke, closed_fill, el.stroke_width)
        }
        "diamond" | "diamond_outline" if vals.len() >= 8 => {
            let points = vec![
                Point2D::new(vals[0], vals[1]),
                Point2D::new(vals[2], vals[3]),
                Point2D::new(vals[4], vals[5]),
                Point2D::new(vals[6], vals[7]),
            ];
            let drawable = generator.polygon::<f64>(&points, &Some(options));
            lower_filled_cap(&drawable, stroke, closed_fill, el.stroke_width)
        }
        _ => vec![],
    }
}

/// Fill along the rough outline itself so small hollow caps mask their shafts
/// without a separately jittered fill polygon.
fn lower_filled_cap(
    drawable: &RoughDrawable<f64>,
    stroke: Rgba,
    fill: Rgba,
    stroke_width: f64,
) -> Vec<PaintedPath> {
    let mut ops = Vec::new();
    for set in drawable
        .sets
        .iter()
        .filter(|set| set.op_set_type == OpSetType::Path)
    {
        for op in convert_ops(set) {
            ops.push(match op {
                PathOp::Move(x, y) if !ops.is_empty() => PathOp::Line(x, y),
                _ => op,
            });
        }
    }
    let mut paths = Vec::new();
    if !ops.is_empty() {
        paths.push(PaintedPath {
            ops,
            fill: Some(fill),
            stroke: None,
            stroke_width: 0.0,
            dash: None,
            fill_even_odd: false,
        });
    }
    paths.extend(lower_drawable(
        drawable,
        Some(stroke),
        None,
        stroke_width,
        None,
    ));
    paths
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

#[derive(Default)]
struct ImageAssets {
    by_file_id: HashMap<String, Option<Arc<ImageAsset>>>,
    next_id: u64,
}

impl ImageAssets {
    fn resolve(
        &mut self,
        file_id: &str,
        files: &HashMap<String, Value>,
    ) -> Option<Arc<ImageAsset>> {
        let next_id = &mut self.next_id;
        self.by_file_id
            .entry(file_id.to_owned())
            .or_insert_with(|| {
                let data_url = files.get(file_id)?.get("dataURL")?.as_str()?;
                if !is_allowed_raster_data_url(data_url) {
                    return None;
                }
                let asset = Arc::new(ImageAsset {
                    id: *next_id,
                    data_url: data_url.to_owned(),
                });
                *next_id += 1;
                Some(asset)
            })
            .clone()
    }
}

fn build_image(
    el: &ExcalidrawElement,
    files: &HashMap<String, Value>,
    image_assets: &mut ImageAssets,
) -> Option<ImagePrimitive> {
    if el.width <= 0.0 || el.height <= 0.0 {
        return None;
    }
    let file_id = el.file_id.as_deref()?;
    let asset = image_assets.resolve(file_id, files)?;
    // Negative scale components flip the image on that axis.
    let flip_x = el
        .scale
        .as_ref()
        .and_then(|s| s.first())
        .is_some_and(|v| *v < 0.0);
    let flip_y = el
        .scale
        .as_ref()
        .and_then(|s| s.get(1))
        .is_some_and(|v| *v < 0.0);
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
        asset,
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
    let stroke_width = drawable
        .options
        .stroke_width
        .map_or(stroke_width, f64::from);
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

    #[test]
    fn outline_only_shaft_matches_the_painted_outline() {
        let mut curved = element("arrow", 100.0, 100.0, true);
        curved.points = Some(vec![(0.0, 0.0), (40.0, 100.0), (100.0, 0.0)]);
        curved.seed = 42;
        let mut closed = element("line", 100.0, 100.0, false);
        closed.points = Some(vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 0.0)]);
        closed.background_color = "#ff0000".into();
        closed.fill_style = "hachure".into();
        closed.seed = 42;

        for el in [curved, closed] {
            let generator = Generator::default();
            let (painted, _) = generate_linear_shaft(&generator, &el, false).unwrap();
            let (outline, _) = generate_linear_shaft(&generator, &el, true).unwrap();
            assert!(
                outline
                    .sets
                    .iter()
                    .all(|set| set.op_set_type == OpSetType::Path)
            );
            let path_ops = |drawable: &RoughDrawable<f64>| {
                drawable
                    .sets
                    .iter()
                    .filter(|set| set.op_set_type == OpSetType::Path)
                    .map(convert_ops)
                    .collect::<Vec<_>>()
            };
            assert_eq!(path_ops(&outline), path_ops(&painted));
        }
    }
}
