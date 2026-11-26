use crate::models::{ExcalidrawElement, ViewBox};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::Path;
use tiny_skia::Pixmap;

/// Write beside the destination, then replace its directory entry only on success.
#[doc(hidden)]
pub fn write_output(path: &Path, write: impl FnOnce(&mut dyn Write) -> Result<()>) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_file() || meta.is_symlink(),
            "Output is not a regular file: {path:?}"
        ),
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => bail!("Cannot inspect output {path:?}: {err}"),
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to create temporary output beside {path:?}"))?;
    let mut writer = BufWriter::new(temp.as_file_mut());
    write(&mut writer)?;
    writer.flush().context("Failed to flush output")?;
    drop(writer);
    temp.persist(path)
        .with_context(|| format!("Failed to replace output {path:?}"))?;
    Ok(())
}

/// Save a pixmap to PNG, replacing the destination only after encoding succeeds.
/// Replaces an output symlink itself, not its target.
///
/// Maps 0-100 to PNG compression types:
/// - 0-25: Fast (fastest encoding, larger files)
/// - 26-75: Default (balanced)
/// - 76-100: Best (slowest encoding, smallest files)
pub fn save_png_with_quality(pixmap: &Pixmap, output_path: &Path, quality: u8) -> Result<()> {
    write_output(output_path, |out| encode_png(pixmap, out, quality))
}

fn encode_png(pixmap: &Pixmap, out: impl Write, quality: u8) -> Result<()> {
    let mut encoder = png::Encoder::new(out, pixmap.width(), pixmap.height());
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

    let mut writer = encoder
        .write_header()
        .context("Failed to write PNG header")?;

    // tiny-skia stores premultiplied RGBA; PNG expects straight (non-premultiplied)
    // alpha. Demultiply every pixel so semi-transparent colors aren't darkened.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    writer
        .write_image_data(&rgba)
        .context("Failed to write PNG data")?;
    writer.finish().context("Failed to finish PNG file")
}

#[cfg(test)]
mod output_tests {
    use super::*;

    #[test]
    fn replaces_output_only_after_success() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("drawing.svg");
        fs::write(&output, "old")?;
        assert!(
            write_output(&output, |out| {
                out.write_all(b"partial")?;
                bail!("render failed")
            })
            .is_err()
        );
        assert_eq!(fs::read(&output)?, b"old");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);

        write_output(&output, |out| Ok(out.write_all(b"complete")?))?;
        assert_eq!(fs::read(output)?, b"complete");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn replaces_symlink_without_changing_target() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("original.svg");
        let output = dir.path().join("drawing.svg");
        fs::write(&target, b"original")?;
        std::os::unix::fs::symlink(&target, &output)?;
        write_output(&output, |out| Ok(out.write_all(b"new")?))?;
        assert_eq!(fs::read(&output)?, b"new");
        assert_eq!(fs::read(target)?, b"original");
        assert!(!fs::symlink_metadata(output)?.is_symlink());
        Ok(())
    }

    #[test]
    fn writes_complete_png() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("drawing.png");
        fs::write(&output, "old")?;
        save_png_with_quality(&Pixmap::new(1, 1).unwrap(), &output, 75)?;
        let bytes = fs::read(output)?;
        let mut reader = png::Decoder::new(bytes.as_slice()).read_info()?;
        reader.next_frame(&mut [0; 4])?;
        reader.finish()?;
        Ok(())
    }
}

pub(crate) fn contributes_bounds(element_type: &str) -> bool {
    // Frames still reserve space even though we don't paint their borders.
    matches!(
        element_type,
        "frame" | "magicframe" | "rectangle" | "ellipse" | "diamond" | "line" | "arrow" | "text" | "image"
    )
}

/// Calculate the viewbox for non-deleted supported elements and frames.
pub fn calculate_viewbox(elements: &[ExcalidrawElement]) -> ViewBox {
    const PADDING: f64 = 40.0;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for el in elements {
        if el.is_deleted || !contributes_bounds(&el.element_type) {
            continue;
        }
        // Linear points can extend left or above their origin. Keep the same
        // stored-box rotation pivot as the renderer, regardless of the bounds.
        let (cx, cy) = crate::math_utils::calculate_center(el.x, el.y, el.width, el.height);
        let (sin, cos) = el.angle.sin_cos();
        let mut include = |px: f64, py: f64| {
            let (dx, dy) = (px - cx, py - cy);
            let rx = cx + dx * cos - dy * sin;
            let ry = cy + dx * sin + dy * cos;
            min_x = min_x.min(rx);
            min_y = min_y.min(ry);
            max_x = max_x.max(rx);
            max_y = max_y.max(ry);
        };
        match (el.element_type.as_str(), el.points.as_deref()) {
            ("line" | "arrow", Some(points)) if !points.is_empty() => {
                // shortcut: padding covers normal overshoot; use painted paths for exact bounds.
                for &(x, y) in points {
                    include(el.x + x, el.y + y);
                }
            }
            _ => {
                include(el.x, el.y);
                include(el.x + el.width, el.y);
                include(el.x + el.width, el.y + el.height);
                include(el.x, el.y + el.height);
            }
        }
    }

    if min_x == f64::INFINITY {
        return ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: 800.0,
            height: 600.0,
        };
    }

    ViewBox {
        min_x: min_x - PADDING,
        min_y: min_y - PADDING,
        width: max_x - min_x + PADDING * 2.0,
        height: max_y - min_y + PADDING * 2.0,
    }
}
