use excaliosa::models::{ArrowheadField, ExcalidrawElement, ViewBox};
use excaliosa::scene::{PaintedPath, Primitive, Rgba, TextPrimitive};
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use std::error::Error;

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/legacy_linear_import.json")).unwrap()
}

fn paths(drawable: &excaliosa::scene::Drawable) -> &[PaintedPath] {
    let Primitive::Paths(paths) = &drawable.primitive else {
        panic!("expected linear paths");
    };
    paths
}

fn text_primitive(scene: &excaliosa::scene::Scene) -> &TextPrimitive {
    scene
        .drawables
        .iter()
        .find_map(|drawable| match &drawable.primitive {
            Primitive::Text(text) => Some(text),
            _ => None,
        })
        .unwrap()
}

#[test]
fn import_normalizes_only_legacy_and_incomplete_linear_records() {
    let data = fixture();
    assert_eq!(data.elements[0].element_type, "line");
    assert_eq!(data.elements[0].id, "legacy-draw");
    assert_eq!(data.elements[0].stroke_width, 4.0);
    assert_eq!(
        data.elements[0].points.as_deref(),
        Some(&[(0.0, 0.0), (120.0, 0.0)][..])
    );

    for index in [1, 2, 4, 5] {
        let element = &data.elements[index];
        assert_eq!(
            element.points.as_deref(),
            Some(&[(0.0, 0.0), (element.width, element.height)][..]),
            "{}",
            element.id
        );
    }
    assert_eq!(data.elements[4].start_arrowhead, ArrowheadField::Null);
    assert_eq!(data.elements[4].end_arrowhead, ArrowheadField::Null);
    assert_eq!(data.elements[7].element_type, "freedraw");
    assert_eq!(data.elements[7].points.as_deref(), Some(&[(0.0, 0.0)][..]));

    let malformed = serde_json::from_str::<ExcalidrawData>(
        r#"{"type":"excalidraw","elements":[{"type":"line","points":[[0,"bad"]]}]}"#,
    );
    assert!(
        malformed.is_err(),
        "malformed tuples must remain typed errors"
    );
}

#[test]
fn recovered_records_use_the_normalized_viewport_and_label_geometry() {
    let data = fixture();
    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    assert_eq!(scene.drawables.len(), 8);
    assert!(scene.drawables[2].label_gap.is_some());
    assert_eq!(text_primitive(&scene).x, 70.0);
    assert_eq!(text_primitive(&scene).y, 180.0);
    assert_eq!(paths(&scene.drawables[4]).len(), 1, "null stays capless");
    assert!(!paths(&scene.drawables[5]).is_empty());
    assert!(!paths(&scene.drawables[6]).is_empty());

    let line: ExcalidrawData = serde_json::from_value(serde_json::json!({
        "type": "excalidraw",
        "elements": [{"type": "line", "x": 20, "y": 90, "width": 120, "height": 40}]
    }))
    .unwrap();
    let view = build_scene(&line, None).viewbox;
    assert_eq!(
        (view.min_x, view.min_y, view.width, view.height),
        (-20.0, 50.0, 200.0, 120.0)
    );
}

fn render_fixture() -> Result<(ViewBox, u32, Vec<u8>), Box<dyn Error>> {
    let data = fixture();
    let viewbox = build_scene(&data, Some(Rgba(0, 0, 0, 0))).viewbox;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("legacy.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None)?;
    let mut reader = png::Decoder::new(std::fs::File::open(output)?).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    reader.finish()?;
    Ok((viewbox, info.width, pixels))
}

#[test]
fn recovered_records_have_visible_svg_and_png_geometry() -> Result<(), Box<dyn Error>> {
    let svg = generate_svg(&fixture(), Some((0, 0, 0, 0)));
    assert!(svg.contains("<path"));
    assert_eq!(svg.matches(">Recovered</tspan>").count(), 1);

    let (viewbox, width, pixels) = render_fixture()?;
    for point in [
        (60.0, 30.0),
        (80.0, 110.0),
        (45.0, 190.0),
        (80.0, 250.0),
        (80.0, 310.0),
        (440.0, 30.0),
    ] {
        let x = (point.0 - viewbox.min_x).floor() as usize;
        let y = (point.1 - viewbox.min_y).floor() as usize;
        let offset = (y * width as usize + x) * 4;
        let pixel = &pixels[offset..offset + 4];
        assert!(pixel[3] > 200, "no recovered ink at {point:?}: {pixel:?}");
    }
    Ok(())
}

fn translated_elbow() -> ExcalidrawData {
    serde_json::from_value(serde_json::json!({
        "type": "excalidraw",
        "elements": [{
            "id": "translated",
            "type": "arrow",
            "x": 2_000_000,
            "y": -2_000_000,
            "width": 80,
            "height": 80,
            "points": [[25, 25], [105, 25], [105, 105]],
            "elbowed": true,
            "endArrowhead": null,
            "roughness": 0
        }]
    }))
    .unwrap()
}

#[test]
fn elbow_guard_uses_finite_local_deltas_not_world_translation() {
    let scene = build_scene(&translated_elbow(), None);
    assert_eq!(scene.drawables.len(), 1);
    assert!(!paths(&scene.drawables[0]).is_empty());

    let mutations: [fn(&mut ExcalidrawElement); 7] = [
        |element| element.x = f64::NAN,
        |element| element.y = f64::NAN,
        |element| element.width = f64::NAN,
        |element| element.height = f64::NAN,
        |element| element.angle = f64::NAN,
        |element| element.points.as_mut().unwrap()[1].0 = f64::NAN,
        |element| element.points.as_mut().unwrap()[1].1 = f64::INFINITY,
    ];
    for mutate in mutations {
        let mut data = translated_elbow();
        mutate(&mut data.elements[0]);
        assert!(build_scene(&data, None).drawables.is_empty());
    }
}

#[test]
fn extreme_elbow_is_skipped_without_capturing_its_label_or_png_limits() -> Result<(), Box<dyn Error>>
{
    let data: ExcalidrawData = serde_json::from_value(serde_json::json!({
        "type": "excalidraw",
        "elements": [
            {
                "id": "extreme",
                "type": "arrow",
                "x": 0,
                "y": 0,
                "width": 1_000_001,
                "height": 0,
                "points": [[500_000, 0], [0, 0], [-500_001, 0]],
                "elbowed": true,
                "boundElements": [{"id": "extreme-label", "type": "text"}]
            },
            {
                "id": "extreme-label",
                "type": "text",
                "x": 300,
                "y": 400,
                "width": 80,
                "height": 20,
                "text": "EXTREME",
                "containerId": "extreme"
            }
        ]
    }))?;
    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(
        (text_primitive(&scene).x, text_primitive(&scene).y),
        (300.0, 400.0)
    );
    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    assert!(svg.contains(">EXTREME</tspan>"));
    assert!(!svg.contains("excaliosa-arrow-label-mask"));

    let directory = tempfile::tempdir()?;
    let output = directory.path().join("extreme.png");
    std::fs::write(&output, "unchanged")?;
    let error = render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap_err();
    assert!(error.to_string().contains("Invalid PNG output dimensions"));
    assert_eq!(std::fs::read(output)?, b"unchanged");
    Ok(())
}
