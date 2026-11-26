use std::f64::consts::FRAC_PI_4;

use excaliosa::scene::{LabelGap, PathOp, Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, generate_svg, render_to_png};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/linear_pivot_bounds.json")).unwrap()
}

fn element(data: &ExcalidrawData, id: &str) -> ExcalidrawElement {
    data.elements
        .iter()
        .find(|element| element.id == id)
        .unwrap()
        .clone()
}

fn scene_for(elements: Vec<ExcalidrawElement>) -> excaliosa::scene::Scene {
    let mut data = fixture();
    data.elements = elements;
    build_scene(&data, Some(Rgba(0, 0, 0, 0)))
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn negative_offset_arrow_shares_its_shaft_pivot_with_label_and_viewport() {
    let data = fixture();
    let arrow = element(&data, "negative-arrow");
    let label = element(&data, "negative-label");
    let scene = scene_for(vec![arrow, label]);

    assert_eq!(scene.drawables.len(), 2);
    close(scene.drawables[0].center.0, 150.0);
    close(scene.drawables[0].center.1, 300.0);
    assert_eq!(
        scene.drawables[0].label_gap,
        Some(LabelGap {
            x: 125.0,
            y: 285.0,
            width: 50.0,
            height: 30.0,
        })
    );
    let Primitive::Text(text) = &scene.drawables[1].primitive else {
        panic!("bound label should follow its arrow");
    };
    assert_eq!((text.x, text.y), (130.0, 290.0));

    // The saved label at (5000, 5000) must not affect these bounds.
    close(scene.viewbox.min_x, 90.0);
    close(scene.viewbox.min_y, 210.0);
    close(scene.viewbox.width, 120.0);
    close(scene.viewbox.height, 180.0);

    let mut data = fixture();
    data.elements
        .retain(|element| matches!(element.id.as_str(), "negative-arrow" | "negative-label"));
    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    assert!(svg.contains("<svg viewBox=\"90 210 120 180\""), "{svg}");
    assert!(svg.contains("transform=\"rotate(90 150 300)\""), "{svg}");
}

#[test]
fn transparent_png_probes_cover_shaft_cap_label_gap_and_margin() {
    let mut data = fixture();
    data.elements
        .retain(|element| matches!(element.id.as_str(), "negative-arrow" | "negative-label"));
    let viewbox = build_scene(&data, Some(Rgba(0, 0, 0, 0))).viewbox;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("linear.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();
    let mut reader = png::Decoder::new(std::fs::File::open(output).unwrap())
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let alpha_at = |x: f64, y: f64| {
        let column = (x - viewbox.min_x).floor() as usize;
        let row = (y - viewbox.min_y).floor() as usize;
        pixels[(row * info.width as usize + column) * 4 + 3]
    };

    assert!(alpha_at(150.0, 260.0) > 0, "shaft must be visible");
    assert!(alpha_at(150.0, 250.0) > 0, "cap must rotate with the shaft");
    assert_eq!(alpha_at(150.0, 312.0), 0, "label gap must mask the shaft");
    assert_eq!(
        alpha_at(viewbox.min_x + 2.0, viewbox.min_y + 2.0),
        0,
        "padding must remain transparent"
    );
}

#[test]
fn cubic_and_elbow_fixture_paths_have_generated_finite_pivots() {
    let data = fixture();
    for id in ["seeded-cubic", "elbow"] {
        let scene = scene_for(vec![element(&data, id)]);
        let center = scene.drawables[0].center;
        assert!(center.0.is_finite() && center.1.is_finite());
        assert!(scene.viewbox.width.is_finite() && scene.viewbox.height.is_finite());
        let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
            panic!("linear fixture must lower to paths");
        };
        assert!(
            paths
                .iter()
                .flat_map(|path| &path.ops)
                .any(|op| matches!(op, PathOp::Cubic(..))),
            "{id} should contain curved path operations"
        );
    }
}

#[test]
fn rotated_circle_and_diamond_use_geometric_bounds() {
    let data = fixture();
    let circle = scene_for(vec![element(&data, "rotated-circle")]);
    close(circle.viewbox.min_x, 460.0);
    close(circle.viewbox.min_y, 410.0);
    close(circle.viewbox.width, 180.0);
    close(circle.viewbox.height, 180.0);

    let diamond = scene_for(vec![ExcalidrawElement {
        element_type: "diamond".into(),
        width: 100.0,
        height: 50.0,
        angle: FRAC_PI_4,
        ..Default::default()
    }]);
    let diamond_extent = 25.0 * 2.0_f64.sqrt();
    close(diamond.viewbox.min_x, 10.0 - diamond_extent);
    close(diamond.viewbox.min_y, -15.0 - diamond_extent);
    close(diamond.viewbox.width, 80.0 + diamond_extent * 2.0);
    close(diamond.viewbox.height, 80.0 + diamond_extent * 2.0);

    let tilted_circle = scene_for(vec![ExcalidrawElement {
        element_type: "ellipse".into(),
        x: 500.0,
        y: 450.0,
        width: 100.0,
        height: 100.0,
        angle: FRAC_PI_4,
        ..Default::default()
    }]);
    close(tilted_circle.viewbox.width, circle.viewbox.width);
    close(tilted_circle.viewbox.height, circle.viewbox.height);
}

#[test]
fn oversized_linear_png_fails_before_replacing_the_destination() {
    let mut data = fixture();
    data.elements = vec![ExcalidrawElement {
        element_type: "line".into(),
        roughness: 0.0,
        points: Some(vec![(0.0, 0.0), (9000.0, 0.0)]),
        ..Default::default()
    }];
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("oversized.png");
    std::fs::write(&output, b"previous output").unwrap();

    let error = render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap_err();
    assert!(error.to_string().contains("Invalid PNG output dimensions"));
    assert_eq!(std::fs::read(output).unwrap(), b"previous output");
}
