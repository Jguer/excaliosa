use anyhow::Result;
use std::path::Path;
use tiny_skia::Pixmap;
use crate::models::{ExcalidrawElement, ViewBox};

/// Save a pixmap to PNG with compression quality control (0-100).
/// Maps 0-100 to PNG compression types:
/// - 0-25: Fast (fastest encoding, larger files)
/// - 26-75: Default (balanced)
/// - 76-100: Best (slowest encoding, smallest files)
pub fn save_png_with_quality(
    pixmap: &Pixmap,
    output_path: &Path,
    quality: u8,
) -> Result<()> {
    use std::io::BufWriter;
    use std::fs::File;
    
    let file = File::create(output_path)
        .map_err(|e| anyhow::anyhow!("Failed to create PNG file: {e}"))?;
    let writer = BufWriter::new(file);
    
    let mut encoder = png::Encoder::new(writer, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_filter(png::FilterType::Paeth);
    
    // Map quality 0-100 to compression type
    let compression_type = if quality <= 25 {
        png::Compression::Fast
    } else if quality <= 75 {
        png::Compression::Default
    } else {
        png::Compression::Best
    };
    encoder.set_compression(compression_type);
    
    let mut writer = encoder.write_header()
        .map_err(|e| anyhow::anyhow!("Failed to write PNG header: {e}"))?;

    // tiny-skia stores premultiplied RGBA; PNG expects straight (non-premultiplied)
    // alpha. Demultiply every pixel so semi-transparent colors aren't darkened.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    writer.write_image_data(&rgba)
        .map_err(|e| anyhow::anyhow!("Failed to write PNG data: {e}"))?;

    Ok(())
}

/// Calculate the viewbox that encompasses all non-deleted elements
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
        if el.is_deleted {
            continue;
        }
        // Bound the element's rotated axis-aligned box. `angle` is in radians;
        // for angle 0 this reduces to the plain (x, y, x+w, y+h) extent.
        let (cx, cy) = (el.x + el.width / 2.0, el.y + el.height / 2.0);
        let (sin, cos) = el.angle.sin_cos();
        for (px, py) in [
            (el.x, el.y),
            (el.x + el.width, el.y),
            (el.x + el.width, el.y + el.height),
            (el.x, el.y + el.height),
        ] {
            let (dx, dy) = (px - cx, py - cy);
            let rx = cx + dx * cos - dy * sin;
            let ry = cy + dx * sin + dy * cos;
            min_x = min_x.min(rx);
            min_y = min_y.min(ry);
            max_x = max_x.max(rx);
            max_y = max_y.max(ry);
        }
    }

    ViewBox {
        min_x: min_x - PADDING,
        min_y: min_y - PADDING,
        width: max_x - min_x + PADDING * 2.0,
        height: max_y - min_y + PADDING * 2.0,
    }
}

