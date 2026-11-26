use std::f64::consts::FRAC_PI_2;

use excaliosa::models::{BoundElement, RoundnessType};
use excaliosa::scene::{LabelGap, Primitive, Rgba, TextPrimitive};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, generate_svg, render_to_png};

fn arrow(id: &str, x: f64, y: f64, points: Vec<(f64, f64)>) -> ExcalidrawElement {
    let (width, height) = points
        .iter()
        .fold((0.0_f64, 0.0_f64), |(w, h), p| (w.max(p.0), h.max(p.1)));
    ExcalidrawElement {
        id: id.into(),
        element_type: "arrow".into(),
        x,
        y,
        width,
        height,
        points: Some(points),
        roughness: 0.0,
        stroke_width: 4.0,
        ..Default::default()
    }
}

fn label(id: &str, container_id: Option<&str>, x: f64, y: f64) -> ExcalidrawElement {
    ExcalidrawElement {
        id: id.into(),
        element_type: "text".into(),
        x,
        y,
        width: 40.0,
        height: 20.0,
        text: Some("LABEL".into()),
        container_id: container_id.map(str::to_owned),
        text_align: Some("center".into()),
        ..Default::default()
    }
}

fn bind(arrow: &mut ExcalidrawElement, text: &ExcalidrawElement) {
    arrow.bound_elements = Some(vec![BoundElement {
        id: text.id.clone(),
        element_type: "text".into(),
    }]);
}

fn scene_for(elements: Vec<ExcalidrawElement>) -> excaliosa::scene::Scene {
    let data: ExcalidrawData = serde_json::from_value(serde_json::json!({
        "type": "excalidraw",
        "elements": elements,
    }))
    .unwrap();
    build_scene(&data, Some(Rgba(0, 0, 0, 0)))
}

fn text_primitive<'a>(scene: &'a excaliosa::scene::Scene, content: &str) -> &'a TextPrimitive {
    scene
        .drawables
        .iter()
        .find_map(|drawable| match &drawable.primitive {
            Primitive::Text(text) if text.content == content => Some(text),
            _ => None,
        })
        .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn live_label_follows_arrow_once_and_stale_bounds_are_replaced() {
    let mut arrow = arrow("arrow", 100.0, 100.0, vec![(0.0, 0.0), (200.0, 0.0)]);
    let text = label("label", Some("arrow"), 1_000.0, 1_000.0);
    bind(&mut arrow, &text);
    let scene = scene_for(vec![arrow, text]);

    assert_eq!(scene.drawables.len(), 2);
    assert!(matches!(scene.drawables[0].primitive, Primitive::Paths(_)));
    assert_eq!(
        scene.drawables[0].label_gap,
        Some(LabelGap {
            x: 175.0,
            y: 85.0,
            width: 50.0,
            height: 30.0,
        })
    );
    let text = match &scene.drawables[1].primitive {
        Primitive::Text(text) => text,
        _ => panic!("label must immediately follow its arrow"),
    };
    assert_eq!((text.x, text.y), (180.0, 90.0));
    assert_eq!(
        (
            scene.viewbox.min_x,
            scene.viewbox.min_y,
            scene.viewbox.width,
            scene.viewbox.height,
        ),
        (60.0, 50.0, 280.0, 100.0)
    );
}

#[test]
fn explicit_positions_use_arc_length_and_are_clamped() {
    let base = arrow(
        "unequal",
        0.0,
        0.0,
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 300.0)],
    );
    for (position, expected) in [
        (0.5, (100.0, 100.0)),
        (-1.0, (0.0, 0.0)),
        (2.0, (100.0, 300.0)),
    ] {
        let mut arrow = base.clone();
        let mut text = label("label", Some("unequal"), 900.0, 900.0);
        text.label_position = Some(position);
        bind(&mut arrow, &text);
        let scene = scene_for(vec![arrow, text]);
        let text = text_primitive(&scene, "LABEL");
        close(text.x + 20.0, expected.0);
        close(text.y + 10.0, expected.1);
    }

    let mut default_arrow = base;
    let text = label("default", Some("unequal"), 900.0, 900.0);
    bind(&mut default_arrow, &text);
    let scene = scene_for(vec![default_arrow, text]);
    let text = text_primitive(&scene, "LABEL");
    assert_eq!((text.x + 20.0, text.y + 10.0), (100.0, 0.0));

    let mut even = arrow(
        "even",
        0.0,
        400.0,
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (300.0, 100.0)],
    );
    let text = label("even-label", Some("even"), 0.0, 0.0);
    bind(&mut even, &text);
    let scene = scene_for(vec![even, text]);
    let text = text_primitive(&scene, "LABEL");
    assert_eq!((text.x + 20.0, text.y + 10.0), (100.0, 450.0));

    let mut vertical = arrow("vertical", 50.0, 20.0, vec![(0.0, 0.0), (0.0, 200.0)]);
    let text = label("vertical-label", Some("vertical"), 0.0, 0.0);
    bind(&mut vertical, &text);
    let scene = scene_for(vec![vertical, text]);
    let text = text_primitive(&scene, "LABEL");
    assert_eq!((text.x + 20.0, text.y + 10.0), (50.0, 120.0));
}

#[test]
fn curved_rotated_and_elbow_labels_use_the_arrow_geometry_and_pivot() {
    let mut curved = arrow(
        "curved",
        0.0,
        0.0,
        vec![(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)],
    );
    curved.roundness = Some(RoundnessType {
        roundness_type: 2,
        value: None,
    });
    let mut curved_text = label("curved-label", Some("curved"), 0.0, 0.0);
    curved_text.label_position = Some(0.25);
    bind(&mut curved, &curved_text);
    let scene = scene_for(vec![curved, curved_text]);
    let text = text_primitive(&scene, "LABEL");
    close(text.x + 20.0, 20.580_212);
    close(text.y + 10.0, 53.053_265);

    let mut rotated = arrow("rotated", 100.0, 100.0, vec![(0.0, 0.0), (200.0, 0.0)]);
    rotated.angle = FRAC_PI_2;
    let mut rotated_text = label("rotated-label", Some("rotated"), 0.0, 0.0);
    rotated_text.label_position = Some(0.25);
    bind(&mut rotated, &rotated_text);
    let scene = scene_for(vec![rotated, rotated_text]);
    let text = text_primitive(&scene, "LABEL");
    assert_eq!((text.x + 20.0, text.y + 10.0), (200.0, 50.0));
    assert_eq!(scene.drawables[1].rotation_deg, 0.0);

    let mut elbow = arrow(
        "elbow",
        0.0,
        0.0,
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 300.0)],
    );
    elbow.elbowed = Some(true);
    elbow.roundness = Some(RoundnessType {
        roundness_type: 2,
        value: None,
    });
    let mut elbow_text = label("elbow-label", Some("elbow"), 0.0, 0.0);
    elbow_text.label_position = Some(0.5);
    bind(&mut elbow, &elbow_text);
    let scene = scene_for(vec![elbow, elbow_text]);
    let text = text_primitive(&scene, "LABEL");
    assert_eq!((text.x + 20.0, text.y + 10.0), (100.0, 100.0));
}

#[test]
fn missing_backlinks_resolve_only_when_unambiguous() {
    let one_way = arrow("one-way", 0.0, 0.0, vec![(0.0, 0.0), (100.0, 0.0)]);
    let text = label("one-way-label", Some("one-way"), 500.0, 500.0);
    let scene = scene_for(vec![one_way, text]);
    assert_eq!(scene.drawables.len(), 2);
    assert_eq!(text_primitive(&scene, "LABEL").x, 30.0);

    let mut reverse = arrow("reverse", 0.0, 100.0, vec![(0.0, 0.0), (100.0, 0.0)]);
    let text = label("reverse-label", None, 500.0, 500.0);
    bind(&mut reverse, &text);
    let scene = scene_for(vec![reverse, text]);
    assert_eq!(scene.drawables.len(), 2);
    assert_eq!(text_primitive(&scene, "LABEL").x, 30.0);

    let arrow = arrow("ambiguous", 0.0, 200.0, vec![(0.0, 0.0), (100.0, 0.0)]);
    let first = label("first", Some("ambiguous"), 500.0, 500.0);
    let mut second = label("second", Some("ambiguous"), 600.0, 600.0);
    second.text = Some("SECOND".into());
    let scene = scene_for(vec![arrow, first, second]);
    assert_eq!(scene.drawables[0].label_gap, None);
    assert_eq!(text_primitive(&scene, "LABEL").x, 500.0);
    assert_eq!(text_primitive(&scene, "SECOND").x, 600.0);
}

#[test]
fn conflicting_dangling_deleted_and_empty_bindings_stay_safe() {
    let mut conflict = arrow("conflict", 0.0, 0.0, vec![(0.0, 0.0), (100.0, 0.0)]);
    let target = label("target", Some("conflict"), 500.0, 500.0);
    let other = label("other", Some("missing-owner"), 600.0, 600.0);
    bind(&mut conflict, &other);
    let scene = scene_for(vec![conflict, target, other]);
    assert_eq!(scene.drawables[0].label_gap, None);
    assert_eq!(text_primitive(&scene, "LABEL").x, 500.0);

    let dangling = label("dangling", Some("absent"), 700.0, 700.0);
    let scene = scene_for(vec![dangling]);
    assert_eq!(text_primitive(&scene, "LABEL").x, 700.0);

    let mut deleted_label_arrow = arrow(
        "deleted-label-arrow",
        0.0,
        100.0,
        vec![(0.0, 0.0), (100.0, 0.0)],
    );
    let mut deleted_label = label("deleted-label", Some("deleted-label-arrow"), 0.0, 0.0);
    deleted_label.is_deleted = true;
    bind(&mut deleted_label_arrow, &deleted_label);
    let scene = scene_for(vec![deleted_label_arrow, deleted_label]);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(scene.drawables[0].label_gap, None);

    let mut empty_label_arrow = arrow(
        "empty-label-arrow",
        0.0,
        200.0,
        vec![(0.0, 0.0), (100.0, 0.0)],
    );
    let mut empty_label = label("empty-label", Some("empty-label-arrow"), 0.0, 0.0);
    empty_label.text = Some(String::new());
    bind(&mut empty_label_arrow, &empty_label);
    let scene = scene_for(vec![empty_label_arrow, empty_label]);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(scene.drawables[0].label_gap, None);

    let mut unrenderable = arrow("unrenderable", 0.0, 300.0, vec![]);
    let orphaned = label("orphaned", Some("unrenderable"), 800.0, 800.0);
    bind(&mut unrenderable, &orphaned);
    let scene = scene_for(vec![unrenderable, orphaned]);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(text_primitive(&scene, "LABEL").x, 800.0);
}

#[test]
fn svg_has_a_nonzero_user_space_mask_and_preserves_paint_order() {
    let data: ExcalidrawData =
        serde_json::from_str(include_str!("fixtures/arrow_bound_labels.json")).unwrap();
    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    let mask = svg
        .lines()
        .find(|line| line.starts_with("<mask id=\"excaliosa-arrow-label-mask-"))
        .expect("bound arrow should have a mask");
    assert!(mask.contains("maskUnits=\"userSpaceOnUse\""));
    assert!(mask.contains("maskContentUnits=\"userSpaceOnUse\""));
    assert!(!mask.contains("width=\"0\""));
    assert!(!mask.contains("height=\"0\""));
    assert_eq!(svg.matches(">Horizontal</tspan>").count(), 1);
    let arrow_mask = svg.find("id=\"excaliosa-arrow-label-mask-0\"").unwrap();
    let label = svg.find(">Horizontal</tspan>").unwrap();
    let later = svg.find("fill=\"#0000ff\"").unwrap();
    assert!(arrow_mask < label && label < later);
}

fn solid_rectangle(id: &str, color: &str) -> ExcalidrawElement {
    ExcalidrawElement {
        id: id.into(),
        element_type: "rectangle".into(),
        x: 110.0,
        y: 80.0,
        width: 80.0,
        height: 40.0,
        background_color: color.into(),
        fill_style: "solid".into(),
        stroke_color: "transparent".into(),
        stroke_width: 0.0,
        roughness: 0.0,
        ..Default::default()
    }
}

fn render_pixel(
    elements: Vec<ExcalidrawElement>,
    background: (u8, u8, u8, u8),
    point: (f64, f64),
) -> [u8; 4] {
    let data: ExcalidrawData = serde_json::from_value(serde_json::json!({
        "type": "excalidraw",
        "elements": elements,
    }))
    .unwrap();
    let viewbox = build_scene(&data, Some(Rgba::from_tuple(background))).viewbox;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("labels.png");
    render_to_png(&data, &output, Some(background), 75, None).unwrap();
    let mut reader = png::Decoder::new(std::fs::File::open(output).unwrap())
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let x = (point.0 - viewbox.min_x).floor() as usize;
    let y = (point.1 - viewbox.min_y).floor() as usize;
    let offset = (y * info.width as usize + x) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
fn png_gap_reveals_only_underlying_paint_and_later_marks_stay_later() {
    let make_arrow = || {
        let mut arrow = arrow("arrow", 50.0, 100.0, vec![(0.0, 0.0), (200.0, 0.0)]);
        let text = label("label", Some("arrow"), 1_000.0, 1_000.0);
        bind(&mut arrow, &text);
        (arrow, text)
    };
    let (arrow, text) = make_arrow();
    assert_eq!(
        render_pixel(vec![arrow, text], (0, 0, 0, 0), (128.0, 100.0)),
        [0, 0, 0, 0]
    );

    let (arrow, text) = make_arrow();
    assert_eq!(
        render_pixel(
            vec![solid_rectangle("under", "#ff0000"), arrow, text],
            (0, 0, 0, 0),
            (128.0, 100.0),
        ),
        [255, 0, 0, 255]
    );

    let (arrow, text) = make_arrow();
    assert_eq!(
        render_pixel(vec![arrow, text], (17, 34, 51, 255), (128.0, 100.0)),
        [17, 34, 51, 255]
    );

    let (arrow, text) = make_arrow();
    assert_eq!(
        render_pixel(
            vec![arrow, text, solid_rectangle("later", "#0000ff")],
            (0, 0, 0, 0),
            (150.0, 95.0),
        ),
        [0, 0, 255, 255]
    );
}
