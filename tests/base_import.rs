use anyhow::Result;
use excaliosa::models::ExcalidrawElement;
use excaliosa::scene::{Primitive, Rgba};
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use serde_json::{Value, json};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/base_import.json")).unwrap()
}

fn isolated(element: ExcalidrawElement) -> ExcalidrawData {
    serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": [element]
    }))
    .unwrap()
}

fn decoded_png(data: &ExcalidrawData) -> Result<(u32, Vec<u8>)> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("base-import.png");
    render_to_png(data, &output, Some((0, 0, 0, 0)), 75, None)?;
    let mut decoder = png::Decoder::new(std::fs::File::open(output)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    reader.finish()?;
    Ok((info.width, pixels))
}

#[test]
fn nullable_indexes_and_omitted_paint_fields_restore_without_changing_rust_defaults() {
    let data = fixture();
    assert_eq!(data.elements[0].index, None);
    assert_eq!(data.elements[1].index, None);
    assert_eq!(data.elements[2].index.as_deref(), Some("z9"));
    assert_eq!(
        serde_json::to_value(&data.elements[2]).unwrap()["index"],
        json!("z9")
    );

    for element in &data.elements[..2] {
        assert_eq!(element.fill_style, "solid");
        assert_eq!(element.stroke_width, 2.0);
        assert_eq!(element.seed, 1);
    }
    assert_eq!(data.elements[2].fill_style, "hachure");
    assert_eq!(data.elements[2].stroke_width, 0.0);
    assert_eq!(data.elements[2].seed, 0);

    let default = ExcalidrawElement::default();
    assert_eq!(default.index, None);
    assert_eq!(default.fill_style, "hachure");
    assert_eq!(default.stroke_width, 1.0);
    assert_eq!(default.seed, 0);
}

#[test]
fn omitted_defaults_match_explicit_defaults_in_svg_and_png() -> Result<()> {
    let data = fixture();
    let omitted = data.elements[0].clone();
    let mut explicit = data.elements[1].clone();
    explicit.id = omitted.id.clone();
    explicit.x = omitted.x;
    explicit.y = omitted.y;
    explicit.index = omitted.index.clone();
    let omitted = isolated(omitted);
    let explicit = isolated(explicit);

    let svg = generate_svg(&omitted, Some((0, 0, 0, 0)));
    assert_eq!(svg, generate_svg(&explicit, Some((0, 0, 0, 0))));
    assert!(svg.contains("fill=\"#ffd43b\""));
    let (width, pixels) = decoded_png(&omitted)?;
    assert!(pixels.chunks(4).any(|pixel| pixel == [255, 212, 59, 255]));
    assert_eq!((width, pixels), decoded_png(&explicit)?);
    Ok(())
}

#[test]
fn conflicting_indexes_do_not_change_file_paint_order() -> Result<()> {
    let data = fixture();
    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    assert_eq!(scene.drawables.len(), 4);
    let paint = |index: usize| {
        let Primitive::Paths(paths) = &scene.drawables[index].primitive else {
            panic!("expected rectangle paths");
        };
        paths
            .iter()
            .find_map(|path| path.fill.or(path.stroke))
            .unwrap()
    };
    assert_eq!(paint(2), Rgba(255, 168, 168, 255));
    assert_eq!(paint(3), Rgba(116, 192, 252, 255));

    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    assert!(svg.find("stroke=\"#ffa8a8\"").unwrap() < svg.find("fill=\"#74c0fc\"").unwrap());
    let (width, pixels) = decoded_png(&data)?;
    let x = (400.0 - scene.viewbox.min_x) as usize;
    let y = (60.0 - scene.viewbox.min_y) as usize;
    let offset = (y * width as usize + x) * 4;
    assert_eq!(&pixels[offset..offset + 4], &[116, 192, 252, 255]);
    Ok(())
}

#[test]
fn malformed_index_and_paint_default_types_are_rejected() {
    let element = serde_json::to_value(fixture().elements[0].clone()).unwrap();
    for (field, value) in [
        ("index", json!(1)),
        ("fillStyle", Value::Null),
        ("strokeWidth", json!("2")),
        ("seed", Value::Null),
    ] {
        let mut malformed = element.clone();
        malformed[field] = value;
        assert!(
            serde_json::from_value::<ExcalidrawElement>(malformed).is_err(),
            "{field} accepted a malformed value"
        );
    }
}
