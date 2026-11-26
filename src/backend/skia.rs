//! Rasterizing backend: renders a [`Scene`] to a PNG via tiny-skia.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use skrifa::{
    GlyphId, MetadataProvider, OutlineGlyph,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    raw::FontRef as ReadFontsRef,
};
use tiny_skia::{
    ColorU8, FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, Path as SkPath, PathBuilder,
    Pixmap, PixmapPaint, Rect, Stroke, StrokeDash, Transform,
};

use crate::font_utils::normalize_newlines;
use crate::fonts::bundled_font;
use crate::models::ExcalidrawData;
use crate::scene::arrow_label::resolve_arrow_labels;
use crate::scene::build::build_scene_with_labels;
use crate::scene::{
    FrameClip, ImagePrimitive, LabelGap, PaintedPath, PathOp, Primitive, Rgba, TextAlign,
    TextPrimitive,
};
use crate::utils::{calculate_viewbox_with_labels, save_png_with_quality};

const SOURCE_DPI: f64 = 96.0;
const MAX_RASTER_AXIS: f64 = 8192.0;
const MAX_RASTER_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_PNG_BASE64_BYTES: usize = 24 * 1024 * 1024;
const PNG_DECODER_BUDGET: usize = 8 * 1024 * 1024;

/// Render an Excalidraw document to a PNG file.
///
/// Output and each embedded PNG are limited to 8192 pixels per axis and
/// 16,777,216 pixels total (64 MiB per RGBA buffer). Only embedded, base64-encoded
/// static PNGs are supported, with a 24 MiB encoded payload limit and an 8 MiB
/// best-effort decoder workspace budget (separate from the pixel buffers).
pub fn render_to_png(
    data: &ExcalidrawData,
    output_path: &Path,
    background: Option<(u8, u8, u8, u8)>,
    quality: u8,
    dpi: Option<u32>,
) -> Result<()> {
    ensure!(dpi != Some(0), "PNG DPI must be positive");
    let scale = dpi.map(|d| f64::from(d) / SOURCE_DPI).unwrap_or(1.0);
    let arrow_labels = resolve_arrow_labels(&data.elements);
    let vb = calculate_viewbox_with_labels(&data.elements, &arrow_labels);
    let (width, height) = raster_size(vb.width * scale, vb.height * scale)
        .context("Invalid PNG output dimensions")?;

    // The shared scene skips missing and unsupported images; don't silently omit them in PNG.
    for el in data
        .elements
        .iter()
        .filter(|el| !el.is_deleted && el.element_type == "image")
    {
        let url = el
            .file_id
            .as_ref()
            .and_then(|id| data.files.get(id))
            .and_then(|file| file.get("dataURL"))
            .and_then(|url| url.as_str())
            .with_context(|| format!("Missing embedded data for image {:?}", el.id))?;
        png_payload(url).with_context(|| format!("Invalid raster image {:?}", el.id))?;
        ensure!(
            el.width > 0.0 && el.height > 0.0,
            "Invalid dimensions for image {:?}",
            el.id
        );
    }
    let scene = build_scene_with_labels(data, background.map(Rgba::from_tuple), vb, arrow_labels);
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
    let base = Transform::from_scale(scale as f32, scale as f32)
        .pre_translate(-vb.min_x as f32, -vb.min_y as f32);

    let mut cached_mask: Option<(&FrameClip, Mask)> = None;
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

        let frame_mask = if let Some(clip) = drawable.frame_clip.as_ref() {
            if cached_mask
                .as_ref()
                .is_none_or(|(cached, _)| *cached != clip)
            {
                cached_mask = build_frame_mask(width, height, base, clip).map(|mask| (clip, mask));
            }
            cached_mask.as_ref().map(|(_, mask)| mask)
        } else {
            None
        };
        let mut label_mask = drawable
            .label_gap
            .and_then(|gap| arrow_label_mask(gap, base, width, height));
        if let (Some(label_mask), Some(frame_mask)) = (label_mask.as_mut(), frame_mask) {
            for (label_alpha, frame_alpha) in
                label_mask.data_mut().iter_mut().zip(frame_mask.data())
            {
                *label_alpha = (u16::from(*label_alpha) * u16::from(*frame_alpha) / 255) as u8;
            }
        }
        let mask = label_mask.as_ref().or(frame_mask);
        match &drawable.primitive {
            Primitive::Paths(paths)
                if drawable
                    .frame_opacity
                    .is_some_and(|opacity| (opacity - 1.0).abs() > 1e-9) =>
            {
                for path in paths {
                    draw_painted(&mut pixmap, path, tf, drawable.opacity, mask);
                }
            }
            Primitive::Paths(paths) => draw_paths(&mut pixmap, paths, tf, drawable.opacity, mask),
            Primitive::Text(text) => draw_text(&mut pixmap, text, tf, drawable.opacity, mask),
            Primitive::Image(image) => draw_image(&mut pixmap, image, tf, drawable.opacity, mask)
                .context("Failed to render embedded PNG image")?,
        }
    }

    save_png_with_quality(&pixmap, output_path, quality)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn draw_paths(
    pixmap: &mut Pixmap,
    paths: &[PaintedPath],
    transform: Transform,
    opacity: f64,
    mask: Option<&Mask>,
) {
    if opacity <= 0.0 {
        return;
    }
    if opacity >= 1.0 {
        for path in paths {
            draw_painted(pixmap, path, transform, 1.0, mask);
        }
        return;
    }

    // Composite overlapping paths before applying element opacity, as SVG groups do.
    let mut bounds: Option<Rect> = None;
    for pp in paths {
        let Some(path) = build_path(&pp.ops) else {
            continue;
        };
        let padding = if pp.stroke.is_some() && pp.stroke_width > 0.0 {
            pp.stroke_width as f32 / 2.0 + 1.0
        } else {
            1.0
        };
        let Some(path_bounds) = path.bounds().outset(padding, padding) else {
            continue;
        };
        let Some(path_bounds) = path_bounds.transform(transform) else {
            continue;
        };
        bounds = Some(match bounds {
            Some(current) => Rect::from_ltrb(
                current.left().min(path_bounds.left()),
                current.top().min(path_bounds.top()),
                current.right().max(path_bounds.right()),
                current.bottom().max(path_bounds.bottom()),
            )
            .unwrap_or(current),
            None => path_bounds,
        });
    }
    let Some(bounds) = bounds else {
        return;
    };
    let left = bounds.left().floor().max(0.0) as i32;
    let top = bounds.top().floor().max(0.0) as i32;
    let right = bounds.right().ceil().min(pixmap.width() as f32) as i32;
    let bottom = bounds.bottom().ceil().min(pixmap.height() as f32) as i32;
    if left >= right || top >= bottom {
        return;
    }
    let Some(mut layer) = Pixmap::new((right - left) as u32, (bottom - top) as u32) else {
        return;
    };
    let layer_transform = transform.post_translate(-left as f32, -top as f32);
    for path in paths {
        draw_painted(&mut layer, path, layer_transform, 1.0, None);
    }
    let paint = PixmapPaint {
        opacity: opacity as f32,
        ..PixmapPaint::default()
    };
    pixmap.draw_pixmap(
        left,
        top,
        layer.as_ref(),
        &paint,
        Transform::identity(),
        mask,
    );
}

fn draw_painted(
    pixmap: &mut Pixmap,
    pp: &PaintedPath,
    transform: Transform,
    opacity: f64,
    mask: Option<&Mask>,
) {
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
            pixmap.fill_path(&path, &paint, rule, transform, mask);
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
            pixmap.stroke_path(&path, &paint, &sk, transform, mask);
        }
    }
}

fn arrow_label_mask(gap: LabelGap, base: Transform, width: u32, height: u32) -> Option<Mask> {
    let rect = Rect::from_xywh(
        gap.x as f32,
        gap.y as f32,
        gap.width as f32,
        gap.height as f32,
    )?;
    let mut mask = Mask::new(width, height)?;
    mask.fill_path(&PathBuilder::from_rect(rect), FillRule::Winding, true, base);
    mask.invert();
    Some(mask)
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

fn draw_text(
    pixmap: &mut Pixmap,
    text: &TextPrimitive,
    transform: Transform,
    opacity: f64,
    mask: Option<&Mask>,
) {
    let Ok(font) = ReadFontsRef::new(bundled_font(text.font_family).bytes) else {
        return;
    };
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    let glyph_for = |ch: char| {
        charmap
            .map(ch)
            .or_else(|| (!ch.is_whitespace() && !ch.is_control()).then_some(GlyphId::NOTDEF))
    };
    let size = Size::new(text.font_size as f32);
    let metrics = font.glyph_metrics(size, LocationRef::default());

    let (r, g, b, a) = apply_opacity(text.color, opacity);
    if a == 0 {
        return;
    }
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;

    let content = normalize_newlines(&text.content);
    for (i, line) in content.split('\n').enumerate() {
        let baseline_y = (text.y + text.baseline_offset + i as f64 * text.line_height) as f32;

        let line_width: f32 = line
            .chars()
            .filter_map(glyph_for)
            .filter_map(|g| metrics.advance_width(g))
            .sum();

        let start_x = match text.align {
            TextAlign::Left => text.x,
            TextAlign::Center => text.x + (text.container_width - line_width as f64) / 2.0,
            TextAlign::Right => text.x + text.container_width - line_width as f64,
        } as f32;

        let mut cursor_x = start_x;
        for ch in line.chars() {
            if let Some(glyph_id) = glyph_for(ch) {
                if let Some(outline) = outlines.get(glyph_id) {
                    if let Some(path) =
                        glyph_path(&outline, text.font_size as f32, cursor_x, baseline_y)
                    {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, mask);
                    }
                }
                if let Some(advance) = metrics.advance_width(glyph_id) {
                    cursor_x += advance;
                }
            }
        }
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

fn png_payload(data_url: &str) -> Result<&str> {
    let (meta, payload) = data_url
        .trim()
        .split_once(',')
        .context("Malformed image data URL")?;
    let (mime, options) = meta
        .split_once(';')
        .context("Image data URL must use base64")?;
    ensure!(
        mime.eq_ignore_ascii_case("data:image/png"),
        "Raster backend supports PNG images only"
    );
    ensure!(
        options
            .rsplit(';')
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case("base64")),
        "Image data URL must use base64"
    );
    ensure!(
        payload.len() <= MAX_PNG_BASE64_BYTES,
        "Embedded PNG payload exceeds 24 MiB"
    );
    Ok(payload)
}

fn decode_embedded_png(data_url: &str) -> Result<Pixmap> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_payload(data_url)?)
        .context("Invalid PNG base64 payload")?;
    let mut decoder = png::Decoder::new_with_limits(
        bytes.as_slice(),
        png::Limits {
            bytes: PNG_DECODER_BUDGET,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let (w, h) = decoder.read_header_info()?.size();
    raster_size(f64::from(w), f64::from(h)).context("Invalid embedded PNG dimensions")?;

    let mut reader = decoder.read_info()?;
    ensure!(
        reader.info().animation_control.is_none(),
        "Animated PNG images are not supported"
    );
    let mut samples = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut samples)?;
    reader.finish()?;
    let mut img = Pixmap::new(w, h).context("Failed to allocate embedded PNG pixmap")?;
    for (pixel, sample) in img
        .pixels_mut()
        .iter_mut()
        .zip(samples.chunks_exact(frame.color_type.samples()))
    {
        let (r, g, b, a) = match *sample {
            [r, g, b, a] => (r, g, b, a),
            [r, g, b] => (r, g, b, 255),
            [g, a] => (g, g, g, a),
            [g] if frame.color_type == png::ColorType::Grayscale => (g, g, g, 255),
            _ => bail!("Unexpected PNG pixel format"),
        };
        *pixel = ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    Ok(img)
}

fn draw_image(
    pixmap: &mut Pixmap,
    image: &ImagePrimitive,
    transform: Transform,
    opacity: f64,
    mask: Option<&Mask>,
) -> Result<()> {
    let img = decode_embedded_png(&image.asset.data_url)?;
    let iw = img.width() as f32;
    let ih = img.height() as f32;

    let (sx, sy, tx, ty) = if let Some(crop) = image.crop {
        let crop_x = iw * crop.x as f32;
        let crop_y = ih * crop.y as f32;
        let crop_width = iw * crop.width as f32;
        let crop_height = ih * crop.height as f32;
        ensure!(
            crop_width > 0.0 && crop_height > 0.0,
            "Invalid embedded PNG crop dimensions"
        );

        // Map the selected source window into the element box, then flip within it.
        let sx = image.width as f32 / crop_width;
        let sy = image.height as f32 / crop_height;
        let (sx, tx) = if image.flip_x {
            (-sx, sx * (crop_x + crop_width))
        } else {
            (sx, -sx * crop_x)
        };
        let (sy, ty) = if image.flip_y {
            (-sy, sy * (crop_y + crop_height))
        } else {
            (sy, -sy * crop_y)
        };
        ensure!(
            [sx, sy, tx, ty].into_iter().all(f32::is_finite),
            "Invalid embedded PNG crop scale"
        );
        (sx, sy, tx, ty)
    } else {
        // Preserve the original uncropped placement fast path.
        let sx = image.width as f32 / iw;
        let sy = image.height as f32 / ih;
        let (sx, tx) = if image.flip_x {
            (-sx, image.width as f32)
        } else {
            (sx, 0.0)
        };
        let (sy, ty) = if image.flip_y {
            (-sy, image.height as f32)
        } else {
            (sy, 0.0)
        };
        (sx, sy, tx, ty)
    };

    let placement = Transform::from_translate(image.x as f32, image.y as f32)
        .pre_translate(tx, ty)
        .pre_scale(sx, sy);
    let img_tf = transform.pre_concat(placement);

    let mut paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    paint.opacity = opacity.clamp(0.0, 1.0) as f32;

    let mut clip = image_clip(pixmap, image, transform)?;
    if let (Some(image_mask), Some(frame_mask)) = (clip.as_mut(), mask) {
        for (image_alpha, frame_alpha) in image_mask.data_mut().iter_mut().zip(frame_mask.data()) {
            *image_alpha = (u16::from(*image_alpha) * u16::from(*frame_alpha) / 255) as u8;
        }
    }
    let clip = clip.as_ref().or(mask);
    pixmap.draw_pixmap(0, 0, img.as_ref(), &paint, img_tf, clip);
    Ok(())
}

fn image_clip(
    pixmap: &Pixmap,
    image: &ImagePrimitive,
    transform: Transform,
) -> Result<Option<Mask>> {
    if image.crop.is_none() && image.corner_radius == 0.0 {
        return Ok(None);
    }
    let rect = Rect::from_xywh(
        image.x as f32,
        image.y as f32,
        image.width as f32,
        image.height as f32,
    )
    .context("Invalid embedded PNG clip geometry")?;
    let path = if image.corner_radius > 0.0 {
        let r = image.corner_radius as f32;
        let c = r * 0.552_284_8;
        let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
        let mut pb = PathBuilder::new();
        pb.move_to(left + r, top);
        pb.line_to(right - r, top);
        pb.cubic_to(right - r + c, top, right, top + r - c, right, top + r);
        pb.line_to(right, bottom - r);
        pb.cubic_to(
            right,
            bottom - r + c,
            right - r + c,
            bottom,
            right - r,
            bottom,
        );
        pb.line_to(left + r, bottom);
        pb.cubic_to(left + r - c, bottom, left, bottom - r + c, left, bottom - r);
        pb.line_to(left, top + r);
        pb.cubic_to(left, top + r - c, left + r - c, top, left + r, top);
        pb.close();
        pb.finish().context("Invalid embedded PNG rounded clip")?
    } else {
        PathBuilder::from_rect(rect)
    };
    let mut mask = Mask::new(pixmap.width(), pixmap.height())
        .context("Failed to allocate embedded PNG clip")?;
    mask.fill_path(&path, FillRule::Winding, true, transform);
    Ok(Some(mask))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_frame_mask(width: u32, height: u32, base: Transform, clip: &FrameClip) -> Option<Mask> {
    let path = build_path(&clip.ops)?;
    let transform = if clip.rotation_deg == 0.0 {
        base
    } else {
        base.pre_concat(Transform::from_rotate_at(
            clip.rotation_deg as f32,
            clip.center.0 as f32,
            clip.center.1 as f32,
        ))
    };
    let mut mask = Mask::new(width, height)?;
    mask.fill_path(&path, FillRule::Winding, true, transform);
    Some(mask)
}

fn raster_size(width: f64, height: f64) -> Result<(u32, u32)> {
    let axis = |dim: f64| {
        ensure!(
            dim.is_finite() && dim > 0.0 && dim <= MAX_RASTER_AXIS,
            "Raster dimensions must be positive and finite and at most {MAX_RASTER_AXIS} per axis"
        );
        Ok(dim.ceil() as u32)
    };
    let (w, h) = (axis(width)?, axis(height)?);
    ensure!(
        u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS,
        "Raster has more than {MAX_RASTER_PIXELS} pixels"
    );
    Ok((w, h))
}

/// Fold element opacity into a color's alpha channel.
fn apply_opacity(c: Rgba, opacity: f64) -> (u8, u8, u8, u8) {
    let a = (c.3 as f64 * opacity).round().clamp(0.0, 255.0) as u8;
    (c.0, c.1, c.2, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_dimensions_are_checked_before_allocation() -> Result<()> {
        assert_eq!(raster_size(1.1, 2.0)?, (2, 2));
        assert_eq!(raster_size(8192.0, 2048.0)?, (8192, 2048));
        for (w, h) in [(8192.0, 2048.1), (8192.1, 1.0), (0.0, 1.0), (1.0, f64::NAN)] {
            assert!(raster_size(w, h).is_err());
        }
        Ok(())
    }

    #[test]
    fn empty_and_all_deleted_pngs_use_the_scaled_default_viewport() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("drawing.png");
        let mut data: ExcalidrawData = serde_json::from_value(serde_json::json!({
            "type": "excalidraw",
            "elements": [{"type": "rectangle", "width": 40, "height": 30, "isDeleted": true}]
        }))?;
        for elements in [std::mem::take(&mut data.elements), vec![]] {
            data.elements = elements;
            for (dpi, expected) in [(None, (800, 600)), (Some(48), (400, 300))] {
                render_to_png(&data, &output, None, 75, dpi)?;
                let file = std::fs::File::open(&output)?;
                assert_eq!(png::Decoder::new(file).read_header_info()?.size(), expected);
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_rasters_leave_output_unchanged() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("drawing.png");
        std::fs::write(&output, b"previous output")?;
        let mut data: ExcalidrawData = serde_json::from_value(serde_json::json!({
            "type": "excalidraw",
            "elements": [{"type": "image", "fileId": "img", "width": 4, "height": 4}]
        }))?;
        let render =
            |data: &ExcalidrawData, dpi| render_to_png(data, &output, None, 75, dpi).unwrap_err();
        assert!(render(&data, Some(0)).to_string().contains("DPI"));
        assert!(
            render(&data, None)
                .to_string()
                .contains("Missing embedded data")
        );
        for (url, expected) in [
            ("data:image/jpeg;base64,AA==", "PNG images only"),
            (
                "data:image/png;base64,aGVsbG8=",
                "Failed to render embedded PNG",
            ),
        ] {
            data.files
                .insert("img".to_owned(), serde_json::json!({ "dataURL": url }));
            assert!(format!("{:#}", render(&data, None)).contains(expected));
        }
        assert_eq!(std::fs::read(output)?, b"previous output");
        Ok(())
    }

    #[test]
    fn embedded_png_preserves_translucent_pixels() -> Result<()> {
        let mut image = Pixmap::new(2, 1).unwrap();
        image.fill(tiny_skia::Color::from_rgba8(0, 255, 0, 128));
        let payload = base64::engine::general_purpose::STANDARD.encode(image.encode_png()?);
        assert_eq!(
            decode_embedded_png(&format!("data:image/png;base64,{payload}"))?.data(),
            image.data()
        );
        Ok(())
    }

    #[test]
    fn missing_printable_glyph_draws_notdef() {
        let mut text = TextPrimitive {
            content: "\u{10fffd}".to_owned(),
            x: 0.0,
            y: 0.0,
            baseline_offset: 24.0,
            font_size: 24.0,
            font_family: "Excalifont",
            color: Rgba(0, 0, 0, 255),
            align: TextAlign::Left,
            line_height: 30.0,
            container_width: 40.0,
        };
        for family in ["Excalifont", "Liberation Sans", "Cascadia Code"] {
            text.font_family = family;
            let font = ReadFontsRef::new(bundled_font(family).bytes).unwrap();
            assert!(font.charmap().map('\u{10fffd}').is_none());
            let mut pixmap = Pixmap::new(40, 40).unwrap();
            draw_text(&mut pixmap, &text, Transform::identity(), 1.0, None);
            assert!(
                pixmap.pixels().iter().any(|pixel| pixel.alpha() > 0),
                "{family}"
            );
        }
    }
}
