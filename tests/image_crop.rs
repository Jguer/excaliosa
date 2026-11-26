use anyhow::Result;
use excaliosa::scene::Primitive;
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use serde_json::json;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];

fn assert_color(actual: [u8; 4], expected: [u8; 4], context: &str) {
    let nearest = [RED, GREEN, BLUE, CYAN, MAGENTA, WHITE, BLACK, YELLOW]
        .into_iter()
        .min_by_key(|color| {
            actual
                .iter()
                .zip(color)
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).pow(2))
                .sum::<i32>()
        })
        .unwrap();
    assert_eq!(nearest, expected, "{context}: actual {actual:?}");
}

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/image_crop.json")).unwrap()
}

fn one(id: &str) -> ExcalidrawData {
    let mut data = fixture();
    data.elements = vec![
        data.elements
            .iter()
            .find(|element| element.id == id)
            .unwrap()
            .clone(),
    ];
    data
}

struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    viewbox: excaliosa::models::ViewBox,
    center: (f64, f64),
    angle: f64,
}

fn render(data: &ExcalidrawData) -> Result<Raster> {
    let scene = build_scene(data, None);
    let dir = tempfile::tempdir()?;
    let output = dir.path().join("image.png");
    render_to_png(data, &output, Some((0, 0, 0, 0)), 75, None)?;
    let file = std::fs::File::open(output)?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    reader.finish()?;
    Ok(Raster {
        width: info.width,
        height: info.height,
        pixels,
        viewbox: scene.viewbox,
        center: scene.drawables[0].center,
        angle: data.elements[0].angle,
    })
}

impl Raster {
    fn pixel(&self, x: f64, y: f64) -> [u8; 4] {
        let (sin, cos) = self.angle.sin_cos();
        let (dx, dy) = (x - self.center.0, y - self.center.1);
        let x = self.center.0 + dx * cos - dy * sin - self.viewbox.min_x;
        let y = self.center.1 + dx * sin + dy * cos - self.viewbox.min_y;
        assert!(x >= 0.0 && y >= 0.0 && x < self.width as f64 && y < self.height as f64);
        let offset = (y as u32 * self.width + x as u32) as usize * 4;
        self.pixels[offset..offset + 4].try_into().unwrap()
    }
}

#[test]
fn svg_maps_crop_into_a_user_space_clip_before_flipping() {
    let svg = generate_svg(&one("crop"), Some((0, 0, 0, 0)));
    assert!(svg.contains(
        "<defs><clipPath id=\"excaliosa-image-clip-0\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"40\" y=\"40\" width=\"80\" height=\"40\"/></clipPath></defs>"
    ));
    assert!(svg.contains(
        "<image id=\"excaliosa-image-asset-0\" x=\"0\" y=\"0\" width=\"1\" height=\"1\" href=\"data:image/png;base64,"
    ));
    assert!(svg.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(0 40) scale(160 40)\"/>",
    ));
    let flipped = generate_svg(&one("flip-x"), Some((0, 0, 0, 0)));
    assert!(flipped.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(440 0) scale(-1 1) translate(140 40) scale(160 40)\"/>",
    ));

    let rounded = generate_svg(&one("rounded-crop"), Some((0, 0, 0, 0)));
    assert!(
        rounded
            .contains("<rect x=\"320\" y=\"160\" width=\"80\" height=\"40\" rx=\"10\" ry=\"10\"/>")
    );

    let uncropped = generate_svg(&one("invalid-crop"), Some((0, 0, 0, 0)));
    assert!(!uncropped.contains("clipPath"));
    assert!(uncropped.contains(
        "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(180 160) scale(80 40)\"/>",
    ));
}

#[test]
fn horizontal_crop_allows_rounding_at_the_natural_height() {
    let mut data = one("crop");
    data.elements[0].height = 183.0;
    let crop = data.elements[0].crop.as_mut().unwrap();
    crop["naturalHeight"] = json!(50);
    crop["height"] = json!(183.0 * (50.0 / 183.0));

    let scene = build_scene(&data, None);
    let Primitive::Image(image) = &scene.drawables[0].primitive else {
        panic!("expected image");
    };
    let crop = image.crop.expect("horizontal crop should be preserved");
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (0.25, 0.0, 0.5, 1.0)
    );
}

#[test]
fn png_crop_composes_horizontal_vertical_flips_and_rotation() -> Result<()> {
    for (id, expected) in [
        ("crop", [GREEN, BLUE, MAGENTA, WHITE]),
        ("flip-x", [BLUE, GREEN, WHITE, MAGENTA]),
        ("flip-y", [MAGENTA, WHITE, GREEN, BLUE]),
        ("flip-both-rotated", [WHITE, MAGENTA, BLUE, GREEN]),
    ] {
        let data = one(id);
        let element = &data.elements[0];
        let raster = render(&data)?;
        for ((dx, dy), expected) in [(20.0, 10.0), (60.0, 10.0), (20.0, 30.0), (60.0, 30.0)]
            .into_iter()
            .zip(expected)
        {
            assert_color(
                raster.pixel(element.x + dx, element.y + dy),
                expected,
                &format!("{id} at ({dx}, {dy})"),
            );
        }
    }
    Ok(())
}

#[test]
fn png_rounded_image_corners_are_transparent() -> Result<()> {
    for id in ["rounded", "rounded-crop"] {
        let data = one(id);
        let element = &data.elements[0];
        let raster = render(&data)?;
        assert_eq!(raster.pixel(element.x + 1.0, element.y + 1.0)[3], 0);
        assert_eq!(
            raster.pixel(element.x + element.width - 1.0, element.y + 1.0)[3],
            0
        );
        assert_eq!(
            raster.pixel(
                element.x + element.width / 2.0,
                element.y + element.height / 2.0
            )[3],
            255
        );
    }
    Ok(())
}

#[test]
fn png_rounded_clip_matches_the_circular_svg_arc() -> Result<()> {
    let mut data = one("rounded");
    data.elements[0].width = 128.0;
    data.elements[0].height = 128.0;
    let element = &data.elements[0];
    let raster = render(&data)?;

    assert_eq!(raster.pixel(element.x + 8.0, element.y + 8.0)[3], 0);
    Ok(())
}

#[test]
fn malformed_crop_falls_back_to_the_uncropped_image() -> Result<()> {
    let valid = fixture().elements[0].crop.clone().unwrap();
    let mut zero_width = valid.clone();
    zero_width["width"] = json!(0);
    let mut negative_x = valid.clone();
    negative_x["x"] = json!(-1);
    let mut outside = valid.clone();
    outside["width"] = json!(4);
    let mut too_small_for_raster = valid;
    too_small_for_raster["width"] = json!(1e-50);
    for crop in [
        json!("invalid"),
        json!({"x": 1, "y": 0, "width": 2}),
        zero_width,
        negative_x,
        outside,
        too_small_for_raster,
    ] {
        let mut data = one("crop");
        data.elements[0].crop = Some(crop);
        let scene = build_scene(&data, None);
        let Primitive::Image(image) = &scene.drawables[0].primitive else {
            panic!("expected image");
        };
        assert!(image.crop.is_none());
        render(&data)?;
    }

    let data = one("invalid-crop");
    let element = &data.elements[0];
    let raster = render(&data)?;
    for ((dx, dy), expected) in [
        ((10.0, 10.0), RED),
        ((30.0, 10.0), GREEN),
        ((10.0, 30.0), CYAN),
        ((70.0, 30.0), BLACK),
    ] {
        assert_color(
            raster.pixel(element.x + dx, element.y + dy),
            expected,
            "uncropped fallback",
        );
    }
    Ok(())
}
