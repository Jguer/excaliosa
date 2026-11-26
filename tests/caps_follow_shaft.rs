use excaliosa::arrow_utils::{calc_arrowhead_points_at_size, cubic_point, resolve_arrowhead_size};
use excaliosa::scene::{PaintedPath, PathOp, Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/caps_follow_shaft.json")).unwrap()
}

fn element(data: &ExcalidrawData, id: &str) -> ExcalidrawElement {
    data.elements
        .iter()
        .find(|element| element.id == id)
        .unwrap()
        .clone()
}

fn paths_for(element: ExcalidrawElement) -> Vec<PaintedPath> {
    let mut data = fixture();
    data.elements = vec![element];
    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    let Primitive::Paths(paths) = &scene.drawables[0].primitive else {
        panic!("arrow should lower to paths");
    };
    paths.clone()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

/// Independent test reference for the terminal segment convention.
fn terminal_direction(ops: &[PathOp], start: bool) -> ((f64, f64), (f64, f64)) {
    #[derive(Clone, Copy)]
    enum Segment {
        Line((f64, f64), (f64, f64)),
        Cubic((f64, f64), (f64, f64), (f64, f64), (f64, f64)),
    }

    let mut current = None;
    let mut first = None;
    let mut last = None;
    for op in ops {
        match *op {
            PathOp::Move(x, y) => current = Some((x, y)),
            PathOp::Line(x, y) => {
                if let Some(p0) = current {
                    let segment = Segment::Line(p0, (x, y));
                    first.get_or_insert(segment);
                    last = Some(segment);
                }
                current = Some((x, y));
            }
            PathOp::Cubic(x1, y1, x2, y2, x3, y3) => {
                if let Some(p0) = current {
                    let segment = Segment::Cubic(p0, (x1, y1), (x2, y2), (x3, y3));
                    first.get_or_insert(segment);
                    last = Some(segment);
                }
                current = Some((x3, y3));
            }
            PathOp::Close => current = None,
        }
    }
    let segment = if start { first } else { last }.unwrap();
    match segment {
        Segment::Line(p0, p1) => {
            if start {
                (p1, p0)
            } else {
                (p0, p1)
            }
        }
        Segment::Cubic(p0, p1, p2, p3) => {
            let sample = cubic_point(p0, p1, p2, p3, 0.7);
            (sample, if start { p0 } else { p3 })
        }
    }
}

fn cap_line_ends(path: &PaintedPath) -> ((f64, f64), (f64, f64)) {
    let Some(PathOp::Move(x, y)) = path.ops.first() else {
        panic!("cap should start at its flank: {:?}", path.ops);
    };
    let end = match path.ops.last() {
        Some(PathOp::Line(x, y) | PathOp::Cubic(_, _, _, _, x, y)) => (*x, *y),
        op => panic!("cap should end at its tip: {op:?}"),
    };
    ((*x, *y), end)
}

fn assert_cap(cap: &[PaintedPath], shaft_ops: &[PathOp], element: &ExcalidrawElement, start: bool) {
    let position = if start { "start" } else { "end" };
    let (tail, tip) = terminal_direction(shaft_ops, start);
    let points = element.points.as_deref().unwrap();
    let curved =
        !element.elbowed.unwrap_or(false) && element.roundness.is_some() && points.len() >= 3;
    let size = resolve_arrowhead_size(
        points,
        position,
        "arrow",
        "arrow",
        element.stroke_width,
        curved,
        element.elbowed.unwrap_or(false),
    );
    let expected = calc_arrowhead_points_at_size(
        tail.0,
        tail.1,
        tip.0,
        tip.1,
        "arrow",
        element.stroke_width,
        size,
        0.0,
    );
    assert_eq!(cap.len(), 2);
    for (path, flank) in cap
        .iter()
        .zip([(expected[2], expected[3]), (expected[4], expected[5])])
    {
        let (actual_flank, actual_tip) = cap_line_ends(path);
        close(actual_flank.0, flank.0);
        close(actual_flank.1, flank.1);
        close(actual_tip.0, tip.0);
        close(actual_tip.1, tip.1);
    }
}

fn assert_both_caps_follow_shaft(element: ExcalidrawElement) {
    let paths = paths_for(element.clone());
    let caps = &paths[paths.len() - 4..];
    let shaft_ops = &paths[0].ops;
    assert_cap(&caps[..2], shaft_ops, &element, true);
    assert_cap(&caps[2..], shaft_ops, &element, false);
}

#[test]
fn fixed_seed_curve_attaches_both_caps_to_lowered_terminal_cubics() {
    let element = element(&fixture(), "seeded-curve");
    assert!(matches!(
        paths_for(element.clone())[0]
            .ops
            .iter()
            .find(|op| matches!(op, PathOp::Cubic(..))),
        Some(PathOp::Cubic(..))
    ));
    assert_both_caps_follow_shaft(element);
}

#[test]
fn rounded_elbow_and_rotated_arrow_use_the_same_terminal_rule() {
    for id in ["rounded-elbow", "rotated-labelled"] {
        assert_both_caps_follow_shaft(element(&fixture(), id));
    }
}

#[test]
fn rotated_bound_label_keeps_its_gap_and_shared_transform() {
    let mut data = fixture();
    data.elements
        .retain(|element| matches!(element.id.as_str(), "rotated-labelled" | "rotated-label"));
    let mut unlabelled = fixture();
    unlabelled.elements = vec![element(&data, "rotated-labelled")];
    let expected_center = build_scene(&unlabelled, None).drawables[0].center;
    let scene = build_scene(&data, Some(Rgba(0, 0, 0, 0)));
    assert_eq!(scene.drawables.len(), 2);
    close(scene.drawables[0].rotation_deg, 60.0);
    assert!(scene.drawables[0].label_gap.is_some());
    assert_eq!(scene.drawables[0].center, expected_center);
    assert_eq!(scene.drawables[1].rotation_deg, 0.0);
    let Primitive::Text(text) = &scene.drawables[1].primitive else {
        panic!("bound text should follow its arrow");
    };
    assert_eq!(text.content, "LABEL");
}

#[test]
fn repeated_raw_endpoint_omits_the_unusable_cap_and_stays_finite() {
    let paths = paths_for(element(&fixture(), "repeated-endpoint"));
    assert_eq!(paths.len(), 1, "only the shaft should remain");
    for op in &paths[0].ops {
        let coordinates: &[f64] = match *op {
            PathOp::Move(x, y) | PathOp::Line(x, y) => &[x, y],
            PathOp::Cubic(x1, y1, x2, y2, x3, y3) => &[x1, y1, x2, y2, x3, y3],
            PathOp::Close => &[],
        };
        assert!(coordinates.iter().all(|value| value.is_finite()), "{op:?}");
    }
}
