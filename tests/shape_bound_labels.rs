use excaliosa::models::BoundElement;
use excaliosa::scene::{Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, generate_svg, render_to_png};
use serde_json::json;

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/shape_bound_labels.json")).unwrap()
}

fn scene_for(elements: Vec<ExcalidrawElement>) -> excaliosa::scene::Scene {
    let data: ExcalidrawData = serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": elements,
    }))
    .unwrap();
    build_scene(&data, None)
}

fn text(drawable: &excaliosa::scene::Drawable) -> Option<&str> {
    match &drawable.primitive {
        Primitive::Text(text) => Some(text.content.as_str()),
        _ => None,
    }
}

fn owner(kind: &str, id: &str, text_ids: &[&str]) -> ExcalidrawElement {
    ExcalidrawElement {
        id: id.into(),
        element_type: kind.into(),
        width: 120.0,
        height: 80.0,
        roughness: 0.0,
        bound_elements: Some(
            text_ids
                .iter()
                .map(|id| BoundElement {
                    id: (*id).into(),
                    element_type: "text".into(),
                })
                .collect(),
        ),
        ..Default::default()
    }
}

fn label(id: &str, container_id: Option<&str>, content: &str) -> ExcalidrawElement {
    ExcalidrawElement {
        id: id.into(),
        element_type: "text".into(),
        width: 80.0,
        height: 25.0,
        text: Some(content.into()),
        container_id: container_id.map(str::to_owned),
        ..Default::default()
    }
}

fn unrelated() -> ExcalidrawElement {
    owner("rectangle", "unrelated", &[])
}

fn assert_saved_text_then_paths(
    elements: Vec<ExcalidrawElement>,
    expected_text: &[&str],
    expected_paths: usize,
) {
    let scene = scene_for(elements);
    assert_eq!(scene.drawables.len(), expected_text.len() + expected_paths);
    for (drawable, expected) in scene.drawables.iter().zip(expected_text) {
        assert_eq!(text(drawable), Some(*expected));
    }
    assert!(
        scene.drawables[expected_text.len()..]
            .iter()
            .all(|drawable| matches!(drawable.primitive, Primitive::Paths(_)))
    );
}

#[test]
fn every_bindable_shape_emits_its_saved_label_immediately_after_its_owner() {
    let scene = build_scene(&fixture(), None);
    let order: Vec<Option<&str>> = scene.drawables.iter().map(text).collect();
    assert_eq!(
        order,
        vec![
            None,
            Some("Rectangle\nlabel"),
            None,
            None,
            Some("Ellipse label"),
            None,
            Some("Diamond label"),
            None,
            Some("1 Jan 1970"),
            Some("Sticky label"),
        ]
    );

    let Primitive::Text(rectangle) = &scene.drawables[1].primitive else {
        panic!("rectangle label should be text");
    };
    assert_eq!((rectangle.x, rectangle.y), (40.0, 45.0));
    assert_eq!(rectangle.content, "Rectangle\nlabel");
    assert_eq!(scene.drawables[1].rotation_deg, 0.15_f64.to_degrees());
    let Primitive::Text(ellipse) = &scene.drawables[4].primitive else {
        panic!("ellipse label should be text");
    };
    assert_eq!((ellipse.x, ellipse.y), (270.0, 55.0));
    assert_eq!(scene.drawables[4].rotation_deg, 0.2_f64.to_degrees());
}

#[test]
fn svg_paint_order_moves_labels_without_duplicating_or_reflowing_them() {
    let svg = generate_svg(&fixture(), None);
    let rectangle_shape = svg.find("fill=\"#ffc9c9\"").unwrap();
    let rectangle_label = svg.find(">Rectangle</tspan>").unwrap();
    let second_line = svg.find(">label</tspan>").unwrap();
    let overlay = svg.find("fill=\"#228be6\"").unwrap();
    assert!(rectangle_shape < rectangle_label && rectangle_label < second_line);
    assert!(second_line < overlay, "{svg}");
    assert_eq!(svg.matches(">Rectangle</tspan>").count(), 1);
    assert_eq!(svg.matches(">label</tspan>").count(), 1);

    for (shape_marker, label_marker) in [
        ("fill=\"#b2f2bb\"", ">Ellipse label</tspan>"),
        ("fill=\"#ffec99\"", ">Diamond label</tspan>"),
        ("fill=\"#ffd43b\"", ">Sticky label</tspan>"),
    ] {
        let shape = svg.find(shape_marker).unwrap();
        let label = svg.find(label_marker).unwrap();
        assert!(shape < label, "{svg}");
        assert_eq!(svg.matches(label_marker).count(), 1);
    }
    assert!(svg.find(">1 Jan 1970</tspan>").unwrap() < svg.find(">Sticky label</tspan>").unwrap());
}

#[test]
fn a_later_opaque_overlay_covers_the_rectangle_label_in_png() {
    let data = fixture();
    let viewbox = build_scene(&data, Some(Rgba(0, 0, 0, 0))).viewbox;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("shape-bound-labels.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();

    let mut decoder = png::Decoder::new(std::fs::File::open(output).unwrap());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    for y in 40..100 {
        for x in 35..165 {
            let px = (f64::from(x) - viewbox.min_x) as usize;
            let py = (f64::from(y) - viewbox.min_y) as usize;
            let offset = (py * info.width as usize + px) * 4;
            assert_eq!(
                &pixels[offset..offset + 4],
                &[34, 139, 230, 255],
                "label painted over the overlay at scene ({x}, {y})"
            );
        }
    }
}

#[test]
fn malformed_bindings_leave_live_text_and_unrelated_art_at_saved_order() {
    let mut deleted_owner = owner("rectangle", "deleted-owner", &["deleted-owner-label"]);
    deleted_owner.is_deleted = true;
    assert_saved_text_then_paths(
        vec![
            label(
                "deleted-owner-label",
                Some("deleted-owner"),
                "Deleted owner",
            ),
            deleted_owner,
            unrelated(),
        ],
        &["Deleted owner"],
        1,
    );

    let mut deleted_label = label("deleted-label", Some("owner"), "Deleted label");
    deleted_label.is_deleted = true;
    assert_saved_text_then_paths(
        vec![
            deleted_label,
            owner("ellipse", "owner", &["deleted-label"]),
            unrelated(),
        ],
        &[],
        2,
    );

    assert_saved_text_then_paths(
        vec![label("dangling", Some("absent"), "Dangling"), unrelated()],
        &["Dangling"],
        1,
    );

    assert_saved_text_then_paths(
        vec![
            label("label", Some("duplicate"), "Duplicate owners"),
            owner("rectangle", "duplicate", &["label"]),
            owner("rectangle", "duplicate", &["label"]),
            unrelated(),
        ],
        &["Duplicate owners"],
        3,
    );

    assert_saved_text_then_paths(
        vec![
            label("shared", Some("owner"), "First duplicate"),
            label("shared", Some("owner"), "Second duplicate"),
            owner("diamond", "owner", &["shared"]),
            unrelated(),
        ],
        &["First duplicate", "Second duplicate"],
        2,
    );

    assert_saved_text_then_paths(
        vec![
            label("label", Some("owner"), "Conflicting"),
            owner("rectangle", "owner", &["other"]),
            unrelated(),
        ],
        &["Conflicting"],
        2,
    );

    assert_saved_text_then_paths(
        vec![
            label("label", Some("first"), "Competing owners"),
            owner("rectangle", "first", &["label"]),
            owner("ellipse", "second", &["label"]),
            unrelated(),
        ],
        &["Competing owners"],
        3,
    );

    assert_saved_text_then_paths(
        vec![
            label("first", Some("owner"), "First ambiguous"),
            label("second", Some("owner"), "Second ambiguous"),
            owner("ellipse", "owner", &["first", "second"]),
            unrelated(),
        ],
        &["First ambiguous", "Second ambiguous"],
        2,
    );
}

#[test]
fn paired_text_keeps_its_own_frame_opacity_and_safe_link() {
    let frame = ExcalidrawElement {
        id: "frame".into(),
        element_type: "frame".into(),
        width: 200.0,
        height: 160.0,
        opacity: 50.0,
        ..Default::default()
    };
    let mut shape = owner("rectangle", "shape", &["label"]);
    shape.frame_id = Some("frame".into());
    let mut text_element = label("label", Some("shape"), "Framed label");
    text_element.frame_id = Some("frame".into());
    text_element.link = Some("https://example.test/label".into());
    let elements = vec![frame, shape, text_element];
    let scene = scene_for(elements.clone());
    let label = scene
        .drawables
        .iter()
        .find(|drawable| text(drawable) == Some("Framed label"))
        .unwrap();
    assert_eq!(label.opacity, 0.5);
    assert_eq!(label.link.as_deref(), Some("https://example.test/label"));

    let data: ExcalidrawData = serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": elements,
    }))
    .unwrap();
    let svg = generate_svg(&data, None);
    let anchor = svg
        .split_once("<a href=\"https://example.test/label\">\n")
        .unwrap()
        .1;
    assert!(anchor.contains("<g clip-path="), "{anchor}");
    assert!(anchor.contains("<text "), "{anchor}");
    assert!(anchor.contains("Framed label"));
}
