//! Rasterizing backend: renders a [`Scene`] to a PNG via tiny-skia.

use std::path::Path;

use anyhow::Result;
use base64::Engine;
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    raw::FontRef as ReadFontsRef,
    MetadataProvider, OutlineGlyph,
};
use tiny_skia::{
    FillRule, FilterQuality, LineCap, LineJoin, Paint, Path as SkPath, PathBuilder, Pixmap,
    PixmapPaint, Rect, Stroke, StrokeDash, Transform,
};

use crate::fonts::{CASCADIA_CODE, EXCALIFONT_REGULAR, LIBERATION_SANS_REGULAR};
use crate::models::ExcalidrawData;
use crate::scene::{
    build_scene, ImagePrimitive, PaintedPath, PathOp, Primitive, Rgba, TextAlign, TextPrimitive,
};
use crate::utils::save_png_with_quality;

const SOURCE_DPI: f32 = 96.0;

/// Render an Excalidraw document to a PNG file.
pub fn render_to_png(
    data: &ExcalidrawData,
    output_path: &Path,
    background: Option<(u8, u8, u8, u8)>,
    quality: u8,
    dpi: Option<u32>,
) -> Result<()> {
    let scene = build_scene(data, background.map(Rgba::from_tuple));

    let scale = dpi.map(|d| d as f32 / SOURCE_DPI).unwrap_or(1.0);
    let vb = scene.viewbox;
    // Guard against degenerate/NaN dimensions (NaN casts to 0).
    let width = ((vb.width * scale as f64).ceil() as u32).max(1);
    let height = ((vb.height * scale as f64).ceil() as u32).max(1);

    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

    // Fill background (default opaque white when none is provided).
    if let Some(bg) = scene.background.or(Some(Rgba(255, 255, 255, 255))) {
        if bg.3 > 0 {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                let mut paint = Paint::default();
                paint.set_color_rgba8(bg.0, bg.1, bg.2, bg.3);
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    }

    // Maps scene coordinates to device pixels: (p - viewbox_min) * scale.
    let base = Transform::from_scale(scale, scale).pre_translate(-vb.min_x as f32, -vb.min_y as f32);

    for drawable in &scene.drawables {
        let tf = if drawable.rotation_deg != 0.0 {
            base.pre_concat(Transform::from_rotate_at(
                drawable.rotation_deg as f32,
                drawable.center.0 as f32,
                drawable.center.1 as f32,
            ))
        } else {
            base
        };

        match &drawable.primitive {
            Primitive::Paths(paths) => {
                for pp in paths {
                    draw_painted(&mut pixmap, pp, tf, drawable.opacity);
                }
            }
            Primitive::Text(text) => draw_text(&mut pixmap, text, tf, drawable.opacity),
            Primitive::Image(image) => draw_image(&mut pixmap, image, tf, drawable.opacity),
        }
    }

    save_png_with_quality(&pixmap, output_path, quality)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn draw_painted(pixmap: &mut Pixmap, pp: &PaintedPath, transform: Transform, opacity: f64) {
    let Some(path) = build_path(&pp.ops) else {
        return;
    };

    if let Some(fill) = pp.fill {
        let (r, g, b, a) = apply_opacity(fill, opacity);
        if a > 0 {
            let mut paint = Paint::default();
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;
            let rule = if pp.fill_even_odd {
                FillRule::EvenOdd
            } else {
                FillRule::Winding
            };
            pixmap.fill_path(&path, &paint, rule, transform, None);
        }
    }

    if let Some(stroke) = pp.stroke {
        let (r, g, b, a) = apply_opacity(stroke, opacity);
        if a > 0 && pp.stroke_width > 0.0 {
            let mut paint = Paint::default();
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;

            let mut sk = Stroke {
                width: pp.stroke_width as f32,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            if let Some(dash) = &pp.dash {
                let pattern: Vec<f32> = dash.iter().map(|v| *v as f32).collect();
                sk.dash = StrokeDash::new(pattern, 0.0);
            }
            pixmap.stroke_path(&path, &paint, &sk, transform, None);
        }
    }
}

fn build_path(ops: &[PathOp]) -> Option<SkPath> {
    let mut pb = PathBuilder::new();
    for op in ops {
        match *op {
            PathOp::Move(x, y) => pb.move_to(x as f32, y as f32),
            PathOp::Line(x, y) => pb.line_to(x as f32, y as f32),
            PathOp::Cubic(a, b, c, d, e, f) => {
                pb.cubic_to(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
            }
            PathOp::Close => pb.close(),
        }
    }
    pb.finish()
}

// ---------------------------------------------------------------------------
// Text (skrifa glyph outlining)
// ---------------------------------------------------------------------------

fn draw_text(pixmap: &mut Pixmap, text: &TextPrimitive, transform: Transform, opacity: f64) {
    let Ok(font) = ReadFontsRef::new(font_bytes(text.font_family)) else {
        return;
    };
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    let size = Size::new(text.font_size as f32);
    let metrics = font.glyph_metrics(size, LocationRef::default());

    let (r, g, b, a) = apply_opacity(text.color, opacity);
    if a == 0 {
        return;
    }
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;

    for (i, line) in text.content.split('\n').enumerate() {
        let baseline_y = (text.y + text.baseline_offset + i as f64 * text.line_height) as f32;

        let line_width: f32 = line
            .chars()
            .filter_map(|ch| charmap.map(ch))
            .filter_map(|g| metrics.advance_width(g))
            .sum();

        let start_x = match text.align {
            TextAlign::Left => text.x,
            TextAlign::Center => text.x + (text.container_width - line_width as f64) / 2.0,
            TextAlign::Right => text.x + text.container_width - line_width as f64,
        } as f32;

        let mut cursor_x = start_x;
        for ch in line.chars() {
            if let Some(glyph_id) = charmap.map(ch) {
                if let Some(outline) = outlines.get(glyph_id) {
                    if let Some(path) = glyph_path(&outline, text.font_size as f32, cursor_x, baseline_y)
                    {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                    }
                }
                if let Some(advance) = metrics.advance_width(glyph_id) {
                    cursor_x += advance;
                }
            }
        }
    }
}

fn font_bytes(family: &str) -> &'static [u8] {
    match family {
        "Liberation Sans" => LIBERATION_SANS_REGULAR,
        "Cascadia Code" => CASCADIA_CODE,
        _ => EXCALIFONT_REGULAR,
    }
}

/// Outline a single glyph into a tiny-skia path positioned at `(ox, oy)` (the
/// glyph pen origin, in scene coordinates). The y-axis is flipped because font
/// space is y-up while device space is y-down.
fn glyph_path(outline: &OutlineGlyph<'_>, font_size: f32, ox: f32, oy: f32) -> Option<SkPath> {
    let mut pen = GlyphPathPen {
        ox,
        oy,
        pb: PathBuilder::new(),
    };
    let settings = DrawSettings::unhinted(Size::new(font_size), LocationRef::new(&[]));
    outline.draw(settings, &mut pen).ok()?;
    pen.pb.finish()
}

struct GlyphPathPen {
    ox: f32,
    oy: f32,
    pb: PathBuilder,
}

impl OutlinePen for GlyphPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.ox + x, self.oy - y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.ox + x, self.oy - y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.pb
            .quad_to(self.ox + cx0, self.oy - cy0, self.ox + x, self.oy - y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.ox + cx0,
            self.oy - cy0,
            self.ox + cx1,
            self.oy - cy1,
            self.ox + x,
            self.oy - y,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

fn draw_image(pixmap: &mut Pixmap, image: &ImagePrimitive, transform: Transform, opacity: f64) {
    // Only PNG data URLs are rasterizable without extra decoders.
    let data_url = image.data_url.trim();
    if !data_url.to_ascii_lowercase().starts_with("data:image/png;") {
        eprintln!("Skipping non-PNG image data URL (raster backend supports PNG only)");
        return;
    }
    let Some((meta, b64)) = data_url.split_once(',') else {
        return;
    };
    if !meta.to_ascii_lowercase().contains(";base64") {
        return;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        eprintln!("Failed to base64-decode image data URL");
        return;
    };
    let img = match Pixmap::decode_png(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to decode PNG image: {e}");
            return;
        }
    };

    let iw = img.width() as f32;
    let ih = img.height() as f32;
    if iw <= 0.0 || ih <= 0.0 || image.width <= 0.0 || image.height <= 0.0 {
        return;
    }

    // Scale the source image into the element box, optionally flipping.
    let sx = image.width as f32 / iw;
    let sy = image.height as f32 / ih;
    let (sx, tx) = if image.flip_x { (-sx, image.width as f32) } else { (sx, 0.0) };
    let (sy, ty) = if image.flip_y { (-sy, image.height as f32) } else { (sy, 0.0) };

    let placement = Transform::from_translate(image.x as f32, image.y as f32)
        .pre_translate(tx, ty)
        .pre_scale(sx, sy);
    let img_tf = transform.pre_concat(placement);

    let mut paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    paint.opacity = opacity.clamp(0.0, 1.0) as f32;

    pixmap.draw_pixmap(0, 0, img.as_ref(), &paint, img_tf, None);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fold element opacity into a color's alpha channel.
fn apply_opacity(c: Rgba, opacity: f64) -> (u8, u8, u8, u8) {
    let a = (c.3 as f64 * opacity).round().clamp(0.0, 255.0) as u8;
    (c.0, c.1, c.2, a)
}
