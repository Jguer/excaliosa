use excaliosa::arrow_utils::{
    calc_arrowhead_points, calc_arrowhead_points_at_size, resolve_arrowhead_size, rotate_point,
};
use excaliosa::models::ArrowheadField;
use excaliosa::scene::{PaintedPath, Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene};
use serde_json::json;
use std::collections::HashMap;

fn wrap(elements: Vec<ExcalidrawElement>) -> ExcalidrawData {
    ExcalidrawData {
        data_type: "excalidraw".into(),
        version: Some(2),
        version_nonce: None,
        source: None,
        elements,
        app_state: HashMap::new(),
        files: HashMap::new(),
    }
}

fn arrow() -> ExcalidrawElement {
    ExcalidrawElement {
        element_type: "arrow".into(),
        points: Some(vec![(0.0, 0.0), (100.0, 0.0)]),
        width: 100.0,
        roughness: 0.0,
        start_arrowhead: ArrowheadField::Null,
        end_arrowhead: ArrowheadField::Null,
        ..Default::default()
    }
}

fn cap_paths(element: ExcalidrawElement) -> Vec<PaintedPath> {
    let scene = build_scene(&wrap(vec![element]), None);
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("expected paths");
    };
    paths[1..].to_vec()
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn all_current_cardinality_names_render_at_both_ends() {
    for (kind, expected_paths) in [
        ("cardinality_one", 1),
        ("cardinality_many", 2),
        ("cardinality_one_or_many", 3),
        ("cardinality_exactly_one", 2),
        ("cardinality_zero_or_one", 3),
        ("cardinality_zero_or_many", 4),
    ] {
        for position in ["start", "end"] {
            let mut element = arrow();
            let field = if position == "start" {
                &mut element.start_arrowhead
            } else {
                &mut element.end_arrowhead
            };
            *field = ArrowheadField::Named(kind.into());
            assert_eq!(
                cap_paths(element).len(),
                expected_paths,
                "{position}: {kind}"
            );
        }
    }
}

#[test]
fn legacy_names_match_current_caps() {
    for (legacy, current) in [
        ("dot", "circle"),
        ("crowfoot_one", "cardinality_one"),
        ("crowfoot_many", "cardinality_many"),
        ("crowfoot_one_or_many", "cardinality_one_or_many"),
    ] {
        for position in ["start", "end"] {
            let mut old = arrow();
            let mut new = arrow();
            let (old_field, new_field) = if position == "start" {
                (&mut old.start_arrowhead, &mut new.start_arrowhead)
            } else {
                (&mut old.end_arrowhead, &mut new.end_arrowhead)
            };
            *old_field = ArrowheadField::Omitted;
            *new_field = ArrowheadField::Named(current.into());
            if position == "start" {
                old.start_arrow_type = Some(legacy.into());
            } else {
                old.end_arrow_type = Some(legacy.into());
            }
            assert_eq!(
                format!("{:#?}", cap_paths(old)),
                format!("{:#?}", cap_paths(new)),
                "{position}: {legacy}"
            );
        }
    }
}

#[test]
fn omitted_modern_fields_fall_back_but_null_suppresses_caps() {
    let data: ExcalidrawData = serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": [
            {"type": "arrow", "points": [[0, 0], [100, 0]], "roughness": 0},
            {"type": "arrow", "points": [[0, 0], [100, 0]], "roughness": 0,
             "startArrowhead": null, "endArrowhead": null},
            {"type": "arrow", "points": [[0, 0], [100, 0]], "roughness": 0,
             "startArrowhead": null, "endArrowType": "bar"},
            {"type": "arrow", "points": [[0, 0], [100, 0]], "roughness": 0,
             "startArrowhead": null, "endArrowType": "bar", "endArrowhead": null},
            {"type": "arrow", "points": [[0, 0], [100, 0]], "roughness": 0,
             "startArrowType": "crowfoot_one", "endArrowhead": null}
        ]
    }))
    .unwrap();
    assert!(matches!(
        data.elements[0].end_arrowhead,
        ArrowheadField::Omitted
    ));
    assert!(matches!(
        data.elements[1].end_arrowhead,
        ArrowheadField::Null
    ));

    let scene = build_scene(&data, None);
    let cap_counts = scene.drawables.iter().map(|drawable| {
        let Primitive::Paths(paths) = &drawable.primitive else {
            panic!("expected paths");
        };
        paths.len() - 1
    });
    assert_eq!(cap_counts.collect::<Vec<_>>(), [2, 0, 1, 0, 1]);
}

#[test]
fn closed_head_flanks_rotate_around_the_tip() {
    let triangle = calc_arrowhead_points(0.0, 0.0, 100.0, 0.0, "triangle", 1.0, 100.0);
    let expected_base = (85.0, 0.0);
    let first = rotate_point(
        expected_base.0,
        expected_base.1,
        100.0,
        0.0,
        -25.0_f64.to_radians(),
    );
    let second = rotate_point(
        expected_base.0,
        expected_base.1,
        100.0,
        0.0,
        25.0_f64.to_radians(),
    );
    assert_eq!(
        triangle,
        vec![100.0, 0.0, first.0, first.1, second.0, second.1]
    );

    let diamond = calc_arrowhead_points(0.0, 0.0, 100.0, 0.0, "diamond", 1.0, 100.0);
    assert_close(diamond[4], 76.0);
    assert_close(diamond[5], 0.0);
    assert_close(diamond[2], 100.0 - 12.0 * 25.0_f64.to_radians().cos());
    assert_close(diamond[3], 12.0 * 25.0_f64.to_radians().sin());
}

#[test]
fn short_arrows_open_up_and_wide_strokes_do_not_inflate_caps() {
    let short = calc_arrowhead_points(0.0, 0.0, 100.0, 0.0, "arrow", 1.0, 5.0);
    let size = 2.5;
    let expected = rotate_point(100.0 - size, 0.0, 100.0, 0.0, -30.0_f64.to_radians());
    assert_close(short[2], expected.0);
    assert_close(short[3], expected.1);

    let wide: Vec<f64> = calc_arrowhead_points(0.0, 0.0, 100.0, 0.0, "arrow", 10.0, 100.0);
    assert_close((wide[2] - 100.0).hypot(wide[3]), 25.0);
    let circle = calc_arrowhead_points(0.0, 0.0, 100.0, 0.0, "circle", 10.0, 100.0);
    assert_eq!(circle, [100.0, 0.0, 23.0]);

    assert!(calc_arrowhead_points(0.0, 0.0, 0.0, 0.0, "arrow", 1.0, 100.0).is_empty());
    assert!(
        calc_arrowhead_points_at_size(0.0, 0.0, 100.0, 0.0, "circle", 2.0, 0.0, 0.0).is_empty()
    );
}

#[test]
fn sharp_corners_limit_growth_and_composite_parts_scale_together() {
    let points = [(0.0, 0.0), (100.0, 0.0), (100.0, 5.0)];
    let triangle =
        resolve_arrowhead_size(&points, "end", "triangle", "triangle", 1.0, false, false);
    let expected = (5.0 / 65.0_f64.to_radians().sin()).min(52.5);
    assert_close(triangle, expected);

    let many = resolve_arrowhead_size(
        &points,
        "end",
        "cardinality_many",
        "cardinality_one_or_many",
        1.0,
        false,
        false,
    );
    let one = resolve_arrowhead_size(
        &points,
        "end",
        "cardinality_one",
        "cardinality_one_or_many",
        1.0,
        false,
        false,
    );
    assert_close(one / many, 20.0 / 15.0);
}

#[test]
fn only_dotted_line_caps_receive_the_tight_dash() {
    for style in ["solid", "dashed", "dotted"] {
        for kind in [
            "arrow",
            "bar",
            "cardinality_one",
            "cardinality_many",
            "cardinality_one_or_many",
            "cardinality_exactly_one",
        ] {
            let mut element = arrow();
            element.end_arrowhead = ArrowheadField::Named(kind.into());
            element.stroke_style = style.into();
            element.stroke_width = 2.0;
            let expected = (style == "dotted").then_some(vec![1.5, 6.0]);
            assert!(
                cap_paths(element).iter().all(|path| path.dash == expected),
                "{style}: {kind}"
            );
        }
        for kind in ["circle", "triangle", "diamond"] {
            let mut element = arrow();
            element.end_arrowhead = ArrowheadField::Named(kind.into());
            element.stroke_style = style.into();
            assert!(cap_paths(element).iter().all(|path| path.dash.is_none()));
        }
    }
}

#[test]
fn hollow_fill_uses_scene_background_without_painting_the_page() {
    let mut element = arrow();
    element.end_arrowhead = ArrowheadField::Named("triangle_outline".into());
    let mut data = wrap(vec![element]);
    data.app_state
        .insert("viewBackgroundColor".into(), json!("#102030"));
    let scene = build_scene(&data, None);
    assert_eq!(scene.background, None);
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("expected paths");
    };
    assert!(
        paths
            .iter()
            .any(|path| path.fill == Some(Rgba(16, 32, 48, 255)))
    );

    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("expected paths");
    };
    assert!(paths.iter().any(|path| path.fill == Some(Rgba(0, 0, 0, 0))));

    data.app_state
        .insert("viewBackgroundColor".into(), json!("not-a-color"));
    let scene = build_scene(&data, None);
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("expected paths");
    };
    assert!(
        paths
            .iter()
            .any(|path| path.fill == Some(Rgba(255, 255, 255, 255)))
    );
}

#[test]
fn rough_caps_are_deterministic_from_the_element_seed() {
    let mut first = arrow();
    first.end_arrowhead = ArrowheadField::Named("cardinality_many".into());
    first.roughness = 2.0;
    first.seed = 42;
    let mut second = first.clone();
    second.seed = 43;
    let first_paths = format!("{:#?}", cap_paths(first.clone()));
    assert_eq!(first_paths, format!("{:#?}", cap_paths(first)));
    assert_ne!(first_paths, format!("{:#?}", cap_paths(second)));
}
