use excaliosa::models::{ArrowheadField, RoundnessType};
use excaliosa::scene::{PaintedPath, PathOp, Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, calculate_viewbox};

fn scene_for(
    elements: Vec<ExcalidrawElement>,
    background: Option<Rgba>,
) -> excaliosa::scene::Scene {
    let mut data: ExcalidrawData =
        serde_json::from_str(r#"{"type":"excalidraw","elements":[]}"#).unwrap();
    data.elements = elements;
    build_scene(&data, background)
}

fn paths(primitive: &Primitive) -> &[PaintedPath] {
    let Primitive::Paths(paths) = primitive else {
        panic!("expected paths");
    };
    paths
}

fn arrow(kind: &str) -> ExcalidrawElement {
    ExcalidrawElement {
        element_type: "arrow".into(),
        points: Some(vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]),
        end_arrowhead: ArrowheadField::Named(kind.into()),
        roughness: 0.0,
        ..Default::default()
    }
}

#[test]
fn deleted_only_uses_empty_viewport() {
    let mut deleted = arrow("arrow");
    deleted.is_deleted = true;
    let view = scene_for(vec![deleted], None).viewbox;
    assert_eq!(
        (view.min_x, view.min_y, view.width, view.height),
        (0.0, 0.0, 800.0, 600.0)
    );
}

#[test]
fn bundled_leftward_connector_is_inside_viewport() {
    let data: ExcalidrawData =
        serde_json::from_str(include_str!("../examples/arrows.json")).unwrap();
    let arrow = data
        .elements
        .iter()
        .find(|el| el.id == "lX5itj7T7hkON_9v1FDH_")
        .unwrap();
    let (x, y) = arrow.points.as_ref().unwrap().last().unwrap();
    let view = build_scene(&data, None).viewbox;
    assert!(view.min_x < arrow.x + x && arrow.x + x < view.min_x + view.width);
    assert!(view.min_y < arrow.y + y && arrow.y + y < view.min_y + view.height);
}

#[test]
fn linear_bounds_use_the_generated_shaft_pivot() {
    let element = ExcalidrawElement {
        x: 100.0,
        y: 50.0,
        width: 100.0,
        height: 20.0,
        angle: std::f64::consts::FRAC_PI_2,
        points: Some(vec![(0.0, 0.0), (-100.0, 0.0)]),
        ..arrow("arrow")
    };
    let scene = scene_for(vec![element], None);
    assert_eq!(scene.drawables[0].center, (50.0, 50.0));
    assert!((scene.viewbox.min_x - 10.0).abs() < 1e-9);
    assert!((scene.viewbox.min_y - (-40.0)).abs() < 1e-9);
    assert!((scene.viewbox.width - 80.0).abs() < 1e-9);
    assert!((scene.viewbox.height - 180.0).abs() < 1e-9);
}

#[test]
fn bounds_keep_frames_but_not_unsupported_elements() {
    let frame = ExcalidrawElement {
        element_type: "frame".into(),
        width: 100.0,
        height: 50.0,
        ..Default::default()
    };
    let unsupported = ExcalidrawElement {
        element_type: "selection".into(),
        x: 1_000.0,
        ..Default::default()
    };
    let view = calculate_viewbox(&[frame, unsupported]);
    assert_eq!(
        (view.min_x, view.min_y, view.width, view.height),
        (-40.0, -60.5, 180.0, 150.5)
    );
}

#[test]
fn sharp_arrowhead_follows_its_final_segment() {
    let scene = scene_for(vec![arrow("arrow")], None);
    let heads = &paths(&scene.drawables[0].primitive)[1..];
    assert_eq!(heads.len(), 2);
    let mut flanks = Vec::new();
    for head in heads {
        let Some(PathOp::Move(x, y)) = head.ops.first() else {
            panic!("unexpected open arrowhead: {:?}", head.ops);
        };
        assert!(matches!(
            head.ops.last(),
            Some(PathOp::Cubic(_, _, _, _, 100.0, 100.0))
        ));
        flanks.push((*x, *y));
    }
    assert!(((flanks[0].0 + flanks[1].0) / 2.0 - 100.0).abs() < 1e-9);
    assert!((flanks[0].1 - flanks[1].1).abs() < 1e-9);
    assert!(flanks[0].1 < 100.0);

    let scene = scene_for(vec![arrow("bar")], None);
    let head = paths(&scene.drawables[0].primitive).last().unwrap();
    assert!(matches!(
        head.ops.first(),
        Some(PathOp::Move(85.0, 100.0))
    ));
    assert!(matches!(
        head.ops.last(),
        Some(PathOp::Cubic(_, _, _, _, 115.0, 100.0))
    ));
}

#[test]
fn outline_heads_follow_export_background() {
    let dark = Rgba(20, 20, 20, 255);
    let transparent = Rgba(0, 0, 0, 0);
    let white = Rgba(255, 255, 255, 255);
    for kind in ["circle_outline", "triangle_outline", "diamond_outline"] {
        for (background, expected) in [
            (Some(dark), dark),
            (Some(transparent), transparent),
            (None, white),
        ] {
            let scene = scene_for(vec![arrow(kind)], background);
            let head = paths(&scene.drawables[0].primitive)
                .iter()
                .find(|path| path.fill.is_some())
                .unwrap();
            assert_eq!(head.fill, Some(expected), "{kind}");
            assert!(paths(&scene.drawables[0].primitive)
                .iter()
                .any(|path| path.stroke.is_some()));
        }
    }
}

#[test]
fn cardinality_variants_have_a_bar_a_fork_or_both() {
    let heads = |kind| {
        let scene = scene_for(vec![arrow(kind)], None);
        paths(&scene.drawables[0].primitive)[1..]
            .iter()
            .map(|p| p.ops.clone())
            .collect::<Vec<_>>()
    };
    let one = heads("cardinality_one");
    let many = heads("cardinality_many");
    assert_eq!(one.len(), 1);
    assert!(matches!(
        (one[0].first(), one[0].last()),
        (Some(PathOp::Move(_, y1)), Some(PathOp::Cubic(_, _, _, _, _, y2))) if y1 == y2
    ));
    assert_eq!(many.len(), 2);
    assert!(many.iter().all(|path| path.len() >= 2));
    let composite = heads("cardinality_one_or_many");
    assert_eq!(composite[..2], many);
    assert_eq!(composite.len(), 3);
    let (Some(PathOp::Move(_, plain_y)), Some(PathOp::Move(_, composite_y))) =
        (one[0].first(), composite[2].first())
    else {
        panic!("unexpected cardinality bar");
    };
    assert_eq!(composite_y - plain_y, 5.0);
}

#[test]
fn only_closed_lines_are_filled() {
    for rounded in [false, true] {
        for (kind, last, should_fill) in [
            ("line", (3.0, 4.0), true),
            ("line", (9.0, 0.0), false),
            ("arrow", (0.0, 0.0), false),
        ] {
            let element = ExcalidrawElement {
                element_type: kind.into(),
                background_color: "#ff0000".into(),
                fill_style: "solid".into(),
                points: Some(vec![(0.0, 0.0), (80.0, 0.0), (60.0, 90.0), last]),
                roundness: rounded.then_some(RoundnessType {
                    roundness_type: 2,
                    value: None,
                }),
                ..Default::default()
            };
            let scene = scene_for(vec![element], None);
            assert_eq!(
                paths(&scene.drawables[0].primitive)
                    .iter()
                    .any(|p| p.fill == Some(Rgba(255, 0, 0, 255))),
                should_fill
            );
        }
    }
}

#[test]
fn non_solid_rough_strokes_use_one_thicker_pass() {
    for (style, expected_moves, expected_width) in
        [("solid", 2, 2.0), ("dashed", 1, 2.5), ("dotted", 1, 2.5)]
    {
        let element = ExcalidrawElement {
            stroke_width: 2.0,
            roughness: 1.0,
            stroke_style: style.into(),
            points: Some(vec![(0.0, 0.0), (100.0, 0.0)]),
            ..arrow("arrow")
        };
        let scene = scene_for(vec![element], None);
        let shaft = &paths(&scene.drawables[0].primitive)[0];
        assert_eq!(
            shaft
                .ops
                .iter()
                .filter(|op| matches!(op, PathOp::Move(..)))
                .count(),
            expected_moves
        );
        assert_eq!(shaft.stroke_width, expected_width);
    }
}
