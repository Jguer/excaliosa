//! Full-scene frame outlines, labels, and direct containing-frame lookup.

use std::collections::HashMap;

use skrifa::{
    GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    raw::FontRef,
};

use crate::font_utils::text_metrics;
use crate::fonts::LIBERATION_SANS_REGULAR;
use crate::models::ExcalidrawElement;

use super::{Drawable, FrameClip, PaintedPath, PathOp, Primitive, Rgba, TextAlign, TextPrimitive};

const FRAME_RADIUS: f64 = 8.0;
const FRAME_NAME_FONT_SIZE: f64 = 14.0;
const FRAME_NAME_LINE_HEIGHT: f64 = 1.25;
const FRAME_NAME_OFFSET_Y: f64 = 3.0;

pub fn containing_frame<'a>(
    element: &ExcalidrawElement,
    elements: &HashMap<&'a str, &'a ExcalidrawElement>,
) -> Option<&'a ExcalidrawElement> {
    let id = element.frame_id.as_deref()?;
    let frame = elements.get(id).copied()?;
    (!frame.is_deleted
        && frame.id != element.id
        && matches!(frame.element_type.as_str(), "frame" | "magicframe"))
    .then_some(frame)
}

pub fn frame_clip(
    element: &ExcalidrawElement,
    frame: &ExcalidrawElement,
    elements: &HashMap<&str, &ExcalidrawElement>,
) -> Option<FrameClip> {
    let bound_to_arrow = element.element_type == "text"
        && element
            .container_id
            .as_deref()
            .and_then(|id| elements.get(id))
            .is_some_and(|container| !container.is_deleted && container.element_type == "arrow");
    if element.element_type == "arrow" || bound_to_arrow {
        return None;
    }
    Some(FrameClip {
        ops: rounded_rect_ops(frame.x, frame.y, frame.width, frame.height, FRAME_RADIUS),
        rotation_deg: frame.angle.to_degrees(),
        center: (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0),
    })
}

pub fn build_frame(element: &ExcalidrawElement) -> Vec<Drawable> {
    let mut drawables = Vec::with_capacity(2);
    if let Some(title) = frame_title(element) {
        drawables.push(Drawable {
            primitive: Primitive::Text(title),
            link: None,
            opacity: 1.0,
            rotation_deg: 0.0,
            center: (0.0, 0.0),
            frame_clip: None,
            frame_opacity: None,
        });
    }
    drawables.push(Drawable {
        primitive: Primitive::Paths(vec![PaintedPath::stroke_only(
            rounded_rect_ops(
                element.x,
                element.y,
                element.width,
                element.height,
                FRAME_RADIUS,
            ),
            Rgba(187, 187, 187, 255),
            2.0,
        )]),
        link: None,
        opacity: 1.0,
        rotation_deg: element.angle.to_degrees(),
        center: (
            element.x + element.width / 2.0,
            element.y + element.height / 2.0,
        ),
        frame_clip: None,
        frame_opacity: None,
    });
    drawables
}

pub(crate) fn frame_title(element: &ExcalidrawElement) -> Option<TextPrimitive> {
    let name = frame_name(element)?;
    let (line_height, baseline_offset) = text_metrics(
        "Liberation Sans",
        FRAME_NAME_FONT_SIZE,
        Some(FRAME_NAME_LINE_HEIGHT),
    );
    let lines = name.split('\n').count() as f64;
    Some(TextPrimitive {
        content: name,
        x: element.x,
        y: element.y - FRAME_NAME_OFFSET_Y - line_height * lines,
        baseline_offset,
        font_size: FRAME_NAME_FONT_SIZE,
        font_family: "Liberation Sans",
        color: Rgba(153, 153, 153, 255),
        align: TextAlign::Left,
        line_height,
        container_width: element.width.max(0.0),
    })
}

fn frame_name(element: &ExcalidrawElement) -> Option<String> {
    let name = element.name.clone().unwrap_or_else(|| {
        if element.element_type == "magicframe" {
            "AI Frame".to_owned()
        } else {
            "Frame".to_owned()
        }
    });
    if name.is_empty() {
        return None;
    }

    let font = FontRef::new(LIBERATION_SANS_REGULAR).ok()?;
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(
        Size::new(FRAME_NAME_FONT_SIZE as f32),
        LocationRef::default(),
    );
    let advance = |ch: char| {
        charmap
            .map(ch)
            .or_else(|| (!ch.is_whitespace() && !ch.is_control()).then_some(GlyphId::NOTDEF))
            .and_then(|glyph| metrics.advance_width(glyph))
            .unwrap_or(0.0) as f64
    };
    let line_width = |line: &str| line.chars().map(advance).sum::<f64>();
    let max_width = element.width.max(0.0);
    let widest_line = name.split('\n').map(line_width).fold(0.0, f64::max);
    if widest_line <= max_width {
        return Some(name);
    }

    let ellipsis_width = line_width("...");
    let mut completed_width = 0.0_f64;
    let mut current_width = 0.0_f64;
    let mut prefix_end = None;
    for (index, ch) in name.char_indices() {
        if ch == '\n' {
            completed_width = completed_width.max(current_width);
            current_width = 0.0;
            if completed_width.max(ellipsis_width) <= max_width {
                prefix_end = Some(index + 1);
            }
        } else {
            current_width += advance(ch);
            if completed_width.max(current_width + ellipsis_width) <= max_width {
                prefix_end = Some(index + ch.len_utf8());
            }
        }
        if completed_width.max(current_width) > max_width {
            break;
        }
    }
    prefix_end.map(|end| format!("{}...", &name[..end]))
}

fn rounded_rect_ops(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Vec<PathOp> {
    let r = radius
        .max(0.0)
        .min(width.abs() / 2.0)
        .min(height.abs() / 2.0);
    let k = 0.552_284_749_830_793_6 * r;
    let (right, bottom) = (x + width, y + height);
    vec![
        PathOp::Move(x + r, y),
        PathOp::Line(right - r, y),
        PathOp::Cubic(right - r + k, y, right, y + r - k, right, y + r),
        PathOp::Line(right, bottom - r),
        PathOp::Cubic(
            right,
            bottom - r + k,
            right - r + k,
            bottom,
            right - r,
            bottom,
        ),
        PathOp::Line(x + r, bottom),
        PathOp::Cubic(x + r - k, bottom, x, bottom - r + k, x, bottom - r),
        PathOp::Line(x, y + r),
        PathOp::Cubic(x, y + r - k, x + r - k, y, x + r, y),
        PathOp::Close,
    ]
}
