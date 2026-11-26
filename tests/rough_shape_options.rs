use excaliosa::models::RoundnessType;
use excaliosa::rect_utils::generate_rounded_diamond_path;
use excaliosa::scene::{PathOp, Primitive};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene};

fn diamond(rounded: bool, roughness: f64) -> ExcalidrawElement {
    ExcalidrawElement {
        element_type: "diamond".into(),
        width: 200.0,
        height: 100.0,
        roughness,
        seed: 42,
        roundness: rounded.then_some(RoundnessType {
            roundness_type: 2,
            value: None,
        }),
        ..Default::default()
    }
}

fn ops(element: ExcalidrawElement) -> Vec<Vec<PathOp>> {
    let mut data: ExcalidrawData =
        serde_json::from_str(r#"{"type":"excalidraw","elements":[]}"#).unwrap();
    data.elements = vec![element];
    let scene = build_scene(&data, None);
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("expected paths");
    };
    paths.iter().map(|path| path.ops.clone()).collect()
}

#[test]
fn non_square_diamond_uses_separate_radii_and_vertex_controls() {
    let element = ExcalidrawElement {
        x: 10.0,
        y: 20.0,
        ..diamond(true, 0.0)
    };
    assert_eq!(
        generate_rounded_diamond_path(&element),
        "M 135 32.5 L 185 57.5 C 210 70, 210 70, 185 82.5 L 135 107.5 C 110 120, 110 120, 85 107.5 L 35 82.5 C 10 70, 10 70, 35 57.5 L 85 32.5 C 110 20, 110 20, 135 32.5"
    );
}

#[test]
fn zero_roughness_unrounded_diamond_keeps_its_original_polygon() {
    let paths = ops(diamond(false, 0.0));
    assert!(
        paths
            .iter()
            .flatten()
            .any(|op| matches!(op, PathOp::Move(x, y) if *x == 100.0 && *y == 0.0))
    );
    assert!(!paths.iter().flatten().any(|op| matches!(op, PathOp::Cubic(x1, y1, x2, y2, _, _) if x1 == x2 && y1 == y2 && *x1 == 200.0 && *y1 == 50.0)));

    let rounded = ops(diamond(true, 0.0));
    assert!(
        rounded
            .iter()
            .flatten()
            .any(|op| matches!(op, PathOp::Move(x, y) if *x == 125.0 && *y == 12.5))
    );
    assert!(rounded.iter().flatten().any(|op| matches!(op, PathOp::Cubic(x1, y1, x2, y2, _, _) if *x1 == 200.0 && *y1 == 50.0 && x1 == x2 && y1 == y2)));
}

#[test]
fn rounded_diamond_is_deterministic_with_a_fixed_seed() {
    assert_eq!(ops(diamond(true, 1.0)), ops(diamond(true, 1.0)));
}

#[test]
fn elbow_paths_preserve_the_start_vertex() {
    let arrow = ExcalidrawElement {
        element_type: "arrow".into(),
        width: 100.0,
        height: 100.0,
        points: Some(vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]),
        elbowed: Some(true),
        roughness: 1.0,
        seed: 7,
        ..Default::default()
    };
    assert!(
        ops(arrow)
            .iter()
            .flatten()
            .any(|op| matches!(op, PathOp::Move(x, y) if *x == 0.0 && *y == 0.0))
    );
}
