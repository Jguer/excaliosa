use std::sync::Arc;

use anyhow::Result;
use excaliosa::scene::{ImagePrimitive, Primitive};
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/image_reuse.json")).unwrap()
}

fn images(data: &ExcalidrawData) -> Vec<ImagePrimitive> {
    build_scene(data, None)
        .drawables
        .into_iter()
        .filter_map(|drawable| match drawable.primitive {
            Primitive::Image(image) => Some(image),
            _ => None,
        })
        .collect()
}

#[test]
fn repeated_references_share_assets_and_emit_each_payload_once() {
    let data = fixture();
    let images = images(&data);
    assert_eq!(images.len(), 5);
    for image in &images[1..4] {
        assert!(Arc::ptr_eq(&images[0].asset, &image.asset));
        assert_eq!(images[0].asset.id, image.asset.id);
    }
    assert!(!Arc::ptr_eq(&images[0].asset, &images[4].asset));
    assert_ne!(images[0].asset.id, images[4].asset.id);

    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    assert_eq!(svg.matches("data:image/png;base64,").count(), 2);
    assert_eq!(svg.matches("<image id=\"excaliosa-image-asset-").count(), 2);
    assert_eq!(
        svg.matches("<use href=\"#excaliosa-image-asset-0\"")
            .count(),
        4
    );
    assert_eq!(
        svg.matches("<use href=\"#excaliosa-image-asset-1\"")
            .count(),
        1
    );
    assert!(!svg.contains("asset\"><&'# "));
    assert!(!svg.contains("dW51c2Vk"));

    for line in svg.lines().filter(|line| line.contains("<use ")) {
        let id = line
            .split("href=\"#")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap();
        assert!(svg.contains(&format!("id=\"{id}\"")), "unresolved {id}");
    }
}

#[test]
fn uses_preserve_instance_crops_flips_frames_and_links() {
    let svg = generate_svg(&fixture(), Some((0, 0, 0, 0)));
    assert!(svg.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(170 40) scale(120 60)\"/>"
    ));
    assert!(svg.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(200 0) scale(-1 1) translate(40 140) scale(120 60)\"/>"
    ));
    assert!(svg.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(0 200) scale(1 -1) translate(400 60) scale(160 80)\"/>"
    ));
    assert!(svg.contains("excaliosa-image-clip-"));
    assert!(svg.contains("excaliosa-frame-clip-"));
    assert!(svg.contains("<a href=\"https://example.com/image\">"));
}

#[test]
fn png_consumes_the_shared_encoded_assets() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let output = dir.path().join("image-reuse.png");
    render_to_png(&fixture(), &output, Some((0, 0, 0, 0)), 75, None)?;
    let mut decoder = png::Decoder::new(std::fs::File::open(output)?);
    let (width, height) = decoder.read_header_info()?.size();
    assert!(width > 0 && height > 0);
    Ok(())
}
