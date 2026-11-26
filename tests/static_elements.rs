use excaliosa::models::BoundElement;
use excaliosa::scene::{Primitive, Rgba};
use excaliosa::{
    ExcalidrawData, ExcalidrawElement, build_scene, calculate_viewbox, generate_svg, render_to_png,
};
use serde_json::json;

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/static_elements.json")).unwrap()
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

fn read_rgba_png(path: &std::path::Path) -> (usize, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    let width = frame.width as usize;
    reader.finish().unwrap();
    (width, bytes)
}

fn bound_note(id: &str, text_id: &str) -> ExcalidrawElement {
    ExcalidrawElement {
        id: id.into(),
        element_type: "stickynote".into(),
        width: 200.0,
        height: 150.0,
        bound_elements: Some(vec![BoundElement {
            id: text_id.into(),
            element_type: "text".into(),
        }]),
        ..Default::default()
    }
}

#[test]
fn sticky_note_paints_paper_footer_then_its_saved_bound_text_once() {
    let scene = build_scene(&fixture(), None);
    let note = &scene.drawables[0];
    assert_eq!(note.opacity, 0.7);
    assert!((note.rotation_deg - 0.06_f64.to_degrees()).abs() < 1e-9);
    let Primitive::Paths(paths) = &note.primitive else {
        panic!("sticky note should lower to paths");
    };
    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0].fill, Some(Rgba(0, 0, 0, 41)));
    assert_eq!(paths[1].fill, Some(Rgba(255, 209, 102, 255)));
    assert!(paths[2].stroke.is_some());

    assert_eq!(text(&scene.drawables[1]), Some("1 Jan 2024"));
    assert_eq!(text(&scene.drawables[2]), Some("Saved sticky label"));
    assert_eq!(
        scene
            .drawables
            .iter()
            .filter(|drawable| text(drawable) == Some("Saved sticky label"))
            .count(),
        1
    );
}

#[test]
fn invalid_or_missing_creation_dates_do_not_add_a_footer() {
    for created in [None, Some(f64::NAN), Some(f64::INFINITY)] {
        let scene = scene_for(vec![ExcalidrawElement {
            element_type: "stickynote".into(),
            width: 200.0,
            height: 150.0,
            created,
            ..Default::default()
        }]);
        assert_eq!(scene.drawables.len(), 1);
        assert!(matches!(scene.drawables[0].primitive, Primitive::Paths(_)));
    }
}

#[test]
fn nonnumeric_creation_dates_are_ignored_in_raw_json() {
    let data: ExcalidrawData = serde_json::from_str(
        r#"{"type":"excalidraw","elements":[
            {"type":"stickynote","width":200,"height":150,"created":"yesterday"},
            {"type":"rectangle","width":20,"height":20,"created":{"timestamp":0}}
        ]}"#,
    )
    .unwrap();
    assert!(
        data.elements
            .iter()
            .all(|element| element.created.is_none())
    );
    let scene = build_scene(&data, None);
    assert_eq!(scene.drawables.len(), 2);
    assert!(
        scene
            .drawables
            .iter()
            .all(|drawable| text(drawable).is_none())
    );
}

#[test]
fn unresolved_sticky_text_remains_at_its_file_position() {
    let text_element = ExcalidrawElement {
        id: "label".into(),
        element_type: "text".into(),
        text: Some("Unbound label".into()),
        container_id: Some("missing-note".into()),
        ..Default::default()
    };
    let scene = scene_for(vec![text_element]);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(text(&scene.drawables[0]), Some("Unbound label"));
}

#[test]
fn sticky_binding_does_not_hide_an_unrelated_shape_with_the_same_id() {
    let label = ExcalidrawElement {
        id: "shared".into(),
        element_type: "text".into(),
        text: Some("Bound label".into()),
        container_id: Some("note".into()),
        width: 100.0,
        height: 25.0,
        ..Default::default()
    };
    let rectangle = ExcalidrawElement {
        id: "shared".into(),
        element_type: "rectangle".into(),
        width: 50.0,
        height: 50.0,
        background_color: "#ff0000".into(),
        fill_style: "solid".into(),
        roughness: 0.0,
        ..Default::default()
    };
    let scene = scene_for(vec![label, bound_note("note", "shared"), rectangle]);
    assert_eq!(scene.drawables.len(), 3);
    assert!(scene.drawables.iter().any(|drawable| {
        matches!(&drawable.primitive, Primitive::Paths(paths) if paths.iter().any(|path| path.fill == Some(Rgba(255, 0, 0, 255))))
    }));
}

#[test]
fn ambiguous_sticky_bindings_leave_text_at_its_saved_position_once() {
    let label = ExcalidrawElement {
        id: "shared-label".into(),
        element_type: "text".into(),
        x: 300.0,
        y: 400.0,
        width: 100.0,
        height: 25.0,
        text: Some("Ambiguous label".into()),
        container_id: Some("duplicate-note".into()),
        ..Default::default()
    };
    let scene = scene_for(vec![
        label,
        bound_note("duplicate-note", "shared-label"),
        bound_note("duplicate-note", "shared-label"),
    ]);
    let labels: Vec<_> = scene
        .drawables
        .iter()
        .filter(|drawable| text(drawable) == Some("Ambiguous label"))
        .collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].center, (350.0, 412.5));
    assert_eq!(scene.drawables.len(), 3);
}

#[test]
fn footer_is_omitted_when_saved_label_bounds_intersect_it() {
    let mut note = bound_note("note", "label");
    note.created = Some(0.0);
    let label = ExcalidrawElement {
        id: "label".into(),
        element_type: "text".into(),
        x: 100.0,
        y: 120.0,
        width: 80.0,
        height: 25.0,
        text: Some("Lower-right saved label".into()),
        container_id: Some("note".into()),
        ..Default::default()
    };
    let scene = scene_for(vec![label, note]);
    assert_eq!(scene.drawables.len(), 2);
    assert_eq!(text(&scene.drawables[1]), Some("Lower-right saved label"));
    assert!(
        scene
            .drawables
            .iter()
            .all(|drawable| text(drawable) != Some("1 Jan 1970"))
    );
}

#[test]
fn sticky_note_bounds_include_the_shadow() {
    let note = ExcalidrawElement {
        element_type: "stickynote".into(),
        width: 100.0,
        height: 80.0,
        ..Default::default()
    };
    let view = calculate_viewbox(&[note]);
    assert_eq!(
        (view.min_x, view.min_y, view.width, view.height),
        (-40.0, -40.0, 183.0, 163.0)
    );
}

#[test]
fn embeds_paint_last_without_reordering_other_elements() {
    let scene = build_scene(&fixture(), None);
    let rectangle_index = scene
        .drawables
        .iter()
        .position(|drawable| match &drawable.primitive {
            Primitive::Paths(paths) => paths
                .iter()
                .any(|path| path.fill == Some(Rgba(255, 168, 168, 255))),
            _ => false,
        })
        .unwrap();
    let iframe_index = scene
        .drawables
        .iter()
        .position(|drawable| text(drawable) == Some("IFrame element"))
        .unwrap();
    let embed_index = scene
        .drawables
        .iter()
        .position(|drawable| text(drawable).is_some_and(|label| label.starts_with("<script>")))
        .unwrap();
    assert!(rectangle_index < iframe_index);
    assert!(iframe_index < embed_index);
    let label = text(&scene.drawables[scene.drawables.len() - 1]).unwrap();
    assert!(label.chars().count() <= 23);
    assert!(label.ends_with('…'));
}

#[test]
fn svg_placeholders_are_escaped_static_text() {
    let svg = generate_svg(&fixture(), None);
    assert!(svg.contains("Saved sticky label"), "{svg}");
    assert!(svg.contains("1 Jan 2024"), "{svg}");
    assert!(svg.contains("IFrame element"), "{svg}");
    assert!(svg.contains("&lt;script&gt;"), "{svg}");
    assert!(svg.contains("&amp;"), "{svg}");
    for forbidden in ["<script>", "<foreignObject", "srcdoc", "href=", "<iframe"] {
        assert!(!svg.contains(forbidden), "found {forbidden} in {svg}");
    }
}

#[test]
fn svg_embed_labels_replace_xml_forbidden_characters() {
    let data: ExcalidrawData = serde_json::from_str(
        r#"{"type":"excalidraw","elements":[{
            "type":"embeddable","width":400,"height":100,
            "link":"https://example.test/\ufffe\uffff"
        }]}"#,
    )
    .unwrap();
    let svg = generate_svg(&data, None);
    assert!(!svg.contains(['\u{fffe}', '\u{ffff}']), "{svg}");
}

#[test]
fn rotated_low_opacity_scene_rasters_to_translucent_png() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("static-elements.png");
    render_to_png(&fixture(), &output, Some((0, 0, 0, 0)), 75, None).unwrap();
    let (_, bytes) = read_rgba_png(&output);
    let alpha = || bytes.iter().skip(3).step_by(4);
    assert!(alpha().any(|value| *value == 255));
    assert!(alpha().any(|value| *value > 0 && *value < 255));
}

#[test]
fn translucent_note_interior_applies_opacity_once_over_its_shadow() {
    let note = ExcalidrawElement {
        element_type: "stickynote".into(),
        width: 200.0,
        height: 150.0,
        background_color: "#ffd166".into(),
        roughness: 0.0,
        opacity: 70.0,
        ..Default::default()
    };
    let data: ExcalidrawData = serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": [note],
    }))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("note.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();
    let (width, bytes) = read_rgba_png(&output);
    // The view box has a 40 px margin; this is the note's shadow-covered center.
    let center = (140 * width + 140) * 4;
    assert_eq!(bytes[center + 3], 178);
}
