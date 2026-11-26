//! Backend-agnostic scene intermediate representation.
//!
//! `build_scene` lowers an [`ExcalidrawData`] document into a flat list of
//! [`Drawable`]s expressed as resolved geometry (op-lists), paint, and a
//! per-element transform. Both the SVG and Skia backends consume the *same*
//! scene, so shape geometry is generated exactly once (via `roughr`) and the
//! two backends render identical output.

pub(crate) mod arrow_label;
pub mod build;
pub mod frame;
pub(crate) mod freedraw;
pub(crate) mod linear_geometry;
mod static_elements;

pub use build::build_scene;

use crate::models::ViewBox;
use std::sync::Arc;

/// Straight (non-premultiplied) 8-bit RGBA color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn from_tuple(c: (u8, u8, u8, u8)) -> Self {
        Rgba(c.0, c.1, c.2, c.3)
    }
}

/// A single sub-path drawing command in absolute scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    Move(f64, f64),
    Line(f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

/// Geometry plus how to paint it. A single element may lower to several of
/// these (e.g. a rough fill sketch, a rough outline with multiple passes, and
/// arrowheads).
#[derive(Clone, Debug)]
pub struct PaintedPath {
    pub ops: Vec<PathOp>,
    pub fill: Option<Rgba>,
    pub stroke: Option<Rgba>,
    pub stroke_width: f64,
    /// Dash pattern for the stroke, if any (in scene units).
    pub dash: Option<Vec<f64>>,
    /// Fill rule to use for `fill`. Rough fill polygons need even-odd.
    pub fill_even_odd: bool,
}

impl PaintedPath {
    pub fn stroke_only(ops: Vec<PathOp>, stroke: Rgba, stroke_width: f64) -> Self {
        PaintedPath {
            ops,
            fill: None,
            stroke: Some(stroke),
            stroke_width,
            dash: None,
            fill_even_odd: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// A block of (possibly multi-line) text with layout already resolved.
#[derive(Clone, Debug)]
pub struct TextPrimitive {
    pub content: String,
    /// Left edge of the text container in scene coordinates.
    pub x: f64,
    /// Top of the text container in scene coordinates.
    pub y: f64,
    /// Vertical distance from the container top to the first baseline.
    pub baseline_offset: f64,
    pub font_size: f64,
    pub font_family: &'static str,
    pub color: Rgba,
    pub align: TextAlign,
    /// Line advance in scene units.
    pub line_height: f64,
    pub container_width: f64,
}

/// A source crop, normalized to the image's natural dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// An encoded raster payload shared by all placements of the same file.
#[derive(Clone, Debug)]
pub struct ImageAsset {
    pub id: u64,
    pub data_url: String,
}

/// A raster image referenced by a validated data URL.
#[derive(Clone, Debug)]
pub struct ImagePrimitive {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub asset: Arc<ImageAsset>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub crop: Option<ImageCrop>,
    pub corner_radius: f64,
}

/// Scene-space rectangle excluded from an arrow's own paint for its label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelGap {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub enum Primitive {
    Paths(Vec<PaintedPath>),
    Text(TextPrimitive),
    Image(ImagePrimitive),
}

/// A containing frame's clip in scene coordinates, before its own rotation.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameClip {
    pub ops: Vec<PathOp>,
    pub rotation_deg: f64,
    pub center: (f64, f64),
}

/// One renderable element: a primitive plus shared element-level state
/// (opacity and rotation about the element center).
#[derive(Clone, Debug)]
pub struct Drawable {
    pub primitive: Primitive,
    /// Saved hyperlink for the source element, if any.
    pub link: Option<String>,
    /// Set only on arrows with a resolved, nonempty bound label.
    pub label_gap: Option<LabelGap>,
    /// 0.0..=1.0
    pub opacity: f64,
    /// Rotation in degrees, applied about `center`.
    pub rotation_deg: f64,
    pub center: (f64, f64),
    pub frame_clip: Option<FrameClip>,
    /// Opacity inherited from a containing frame, before combining with the element's opacity.
    pub frame_opacity: Option<f64>,
}

pub struct Scene {
    pub viewbox: ViewBox,
    pub background: Option<Rgba>,
    pub drawables: Vec<Drawable>,
}
