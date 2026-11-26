use excaliosa::models::StrokeOptions;
use excaliosa::scene::{Drawable, PaintedPath, PathOp, Primitive, Rgba};
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, generate_svg, render_to_png};
use std::error::Error;

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/freedraw.json")).unwrap()
}

fn scene_for(element: ExcalidrawElement) -> excaliosa::scene::Scene {
    let mut data = fixture();
    data.elements = vec![element];
    build_scene(&data, None)
}

fn paths(drawable: &Drawable) -> &[PaintedPath] {
    let Primitive::Paths(paths) = &drawable.primitive else {
        panic!("expected freedraw paths");
    };
    paths
}

fn ink_ops(scene: &excaliosa::scene::Scene) -> &[PathOp] {
    &paths(&scene.drawables[0]).last().unwrap().ops
}

fn mark(variability: Option<&str>) -> ExcalidrawElement {
    ExcalidrawElement {
        element_type: "freedraw".into(),
        stroke_width: 4.0,
        roughness: 0.0,
        points: Some(vec![(0.0, 0.0), (30.0, 10.0), (60.0, 0.0)]),
        pressures: Some(vec![1.0, 0.5, 0.0]),
        stroke_options: Some(StrokeOptions {
            variability: variability.map(str::to_owned),
            streamline: Some(0.5),
        }),
        ..Default::default()
    }
}

#[test]
fn fixture_has_a_dot_variable_and_constant_ink_and_loop_fill() {
    let data = fixture();
    assert_eq!(data.elements[0].pressures.as_deref(), Some(&[1.0][..]));
    assert!(!data.elements[0].simulate_pressure);
    assert_eq!(
        data.elements[2]
            .stroke_options
            .as_ref()
            .and_then(|options| options.variability.as_deref()),
        Some("constant")
    );

    let scene = build_scene(&data, None);
    assert_eq!(scene.drawables.len(), 6, "transparent ink is omitted");
    assert_eq!(paths(&scene.drawables[0]).len(), 1);
    assert!(
        paths(&scene.drawables[0])[0]
            .ops
            .iter()
            .all(|op| matches!(op, PathOp::Move(..) | PathOp::Line(..) | PathOp::Close))
    );

    let variable = paths(&scene.drawables[1]).last().unwrap();
    let constant = paths(&scene.drawables[2]).last().unwrap();
    assert_eq!(variable.fill, Some(Rgba(25, 113, 194, 255)));
    assert_eq!(constant.fill, Some(Rgba(224, 49, 49, 255)));
    assert_ne!(variable.ops, constant.ops);

    let loop_paths = paths(&scene.drawables[3]);
    assert_eq!(
        loop_paths.first().unwrap().fill,
        Some(Rgba(255, 212, 59, 255))
    );
    assert_eq!(loop_paths.last().unwrap().fill, Some(Rgba(30, 30, 30, 255)));
    assert!(loop_paths.len() >= 2, "fill is painted below ink");
}

#[test]
fn recorded_pressure_changes_width_but_constant_ink_ignores_it() {
    let variable_a = scene_for(mark(Some("variable")));
    let mut changed = mark(Some("variable"));
    changed.pressures = Some(vec![0.0, 1.0, 1.0]);
    let variable_b = scene_for(changed);
    assert_ne!(ink_ops(&variable_a), ink_ops(&variable_b));

    let mut taper = mark(Some("variable"));
    taper.points = Some(vec![(0.0, 0.0), (0.0, 100.0)]);
    taper.pressures = Some(vec![1.0, 0.0]);
    let taper = scene_for(taper);
    let [PathOp::Move(wide, _), PathOp::Line(thin, _), ..] = ink_ops(&taper) else {
        panic!("unexpected tapered outline");
    };
    assert!(wide.abs() > thin.abs(), "recorded pressure tapers the ink");

    let constant_a = scene_for(mark(Some("constant")));
    let mut changed = mark(Some("constant"));
    changed.pressures = Some(vec![0.0, 1.0, 0.25]);
    let constant_b = scene_for(changed);
    assert_eq!(ink_ops(&constant_a), ink_ops(&constant_b));

    let mut straight = mark(Some("constant"));
    straight.points = Some(vec![(0.0, 0.0), (100.0, 0.0)]);
    let straight = scene_for(straight);
    let (min_y, max_y) =
        ink_ops(&straight)
            .iter()
            .fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(min_y, max_y), op| match *op {
                    PathOp::Move(_, y) | PathOp::Line(_, y) => (min_y.min(y), max_y.max(y)),
                    _ => (min_y, max_y),
                },
            );
    assert!((max_y - min_y - 11.2).abs() < 1e-9);

    let unknown = scene_for(mark(Some("future-mode")));
    let default = scene_for(mark(None));
    assert_eq!(ink_ops(&unknown), ink_ops(&default));
    assert_eq!(ink_ops(&unknown), ink_ops(&variable_a));
}

#[test]
fn malformed_pressure_and_repeated_points_fall_back_safely() {
    let mut malformed: ExcalidrawElement = serde_json::from_str(
        r#"{
            "type": "freedraw",
            "strokeWidth": 4,
            "points": [[0, 0], [0, 0], [30, 10], [30, 10], [60, 0]],
            "pressures": [0.5, null]
        }"#,
    )
    .unwrap();
    assert_eq!(malformed.pressures, None);
    let actual = scene_for(malformed.clone());
    assert!(ink_ops(&actual).iter().all(|op| match *op {
        PathOp::Move(x, y) | PathOp::Line(x, y) => x.is_finite() && y.is_finite(),
        _ => true,
    }));

    malformed.simulate_pressure = true;
    let simulated = scene_for(malformed);
    assert_eq!(ink_ops(&actual), ink_ops(&simulated));

    let mut repeated = mark(None);
    repeated.points = Some(vec![(0.0, 0.0), (0.0, 0.0), (30.0, 10.0), (60.0, 0.0)]);
    repeated.pressures = Some(vec![1.0, 1.0, 0.5, 0.0]);
    let deduplicated = scene_for(mark(None));
    assert_eq!(ink_ops(&scene_for(repeated)), ink_ops(&deduplicated));
}

#[test]
fn out_and_back_stroke_produces_visible_ink() {
    let mut reversed = mark(Some("constant"));
    reversed.points = Some(vec![(0.0, 0.0), (40.0, 0.0), (0.0, 0.0)]);
    reversed.stroke_options.as_mut().unwrap().streamline = Some(0.0);
    let scene = scene_for(reversed);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!(
        paths(&scene.drawables[0])[0].fill,
        Some(Rgba(30, 30, 30, 255))
    );
    assert!(ink_ops(&scene).iter().any(|op| match *op {
        PathOp::Move(x, _) | PathOp::Line(x, _) => x > 40.0,
        _ => false,
    }));
}

#[test]
fn loop_fill_is_inside_the_standalone_viewport() {
    let mut loop_mark = mark(None);
    loop_mark.points = Some(vec![(0.0, 0.0), (100.0, -100.0), (200.0, 0.0), (0.0, 0.0)]);
    loop_mark.background_color = "#ffd43b".into();
    loop_mark.stroke_options.as_mut().unwrap().streamline = Some(0.75);
    let scene = scene_for(loop_mark);
    assert!(paths(&scene.drawables[0]).len() >= 2);
    let mut min_y = f64::INFINITY;
    for op in paths(&scene.drawables[0]).iter().flat_map(|path| &path.ops) {
        let points: &[(f64, f64)] = match *op {
            PathOp::Move(x, y) | PathOp::Line(x, y) => &[(x, y)],
            PathOp::Cubic(x1, y1, x2, y2, x, y) => &[(x1, y1), (x2, y2), (x, y)],
            PathOp::Close => &[],
        };
        for &(_, y) in points {
            min_y = min_y.min(y);
            assert!(y >= scene.viewbox.min_y, "fill clipped at y={y}");
            assert!(y <= scene.viewbox.min_y + scene.viewbox.height);
        }
    }
    assert!(min_y < -90.0);
}

#[test]
fn transparent_off_canvas_mark_does_not_expand_png_viewport() -> Result<(), Box<dyn Error>> {
    let dot = ExcalidrawElement {
        element_type: "freedraw".into(),
        stroke_width: 4.0,
        points: Some(vec![(0.0, 0.0)]),
        pressures: Some(vec![1.0]),
        ..Default::default()
    };
    let invisible = ExcalidrawElement {
        element_type: "freedraw".into(),
        x: 10000.0,
        stroke_color: "transparent".into(),
        background_color: "transparent".into(),
        points: Some(vec![(0.0, 0.0), (100.0, 0.0), (0.0, 0.0)]),
        ..Default::default()
    };
    let mut data = fixture();
    data.elements = vec![dot, invisible];
    let scene = build_scene(&data, None);
    assert_eq!(scene.drawables.len(), 1);
    assert_eq!((scene.viewbox.width, scene.viewbox.height), (97.0, 97.0));

    let directory = tempfile::tempdir()?;
    let output = directory.path().join("transparent.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None)?;
    let decoder = png::Decoder::new(std::fs::File::open(output)?);
    let reader = decoder.read_info()?;
    assert_eq!((reader.info().width, reader.info().height), (97, 97));
    Ok(())
}

#[test]
fn dot_bounds_include_the_painted_outline() {
    let dot = ExcalidrawElement {
        element_type: "freedraw".into(),
        stroke_width: 4.0,
        points: Some(vec![(0.0, 0.0)]),
        pressures: Some(vec![1.0]),
        ..Default::default()
    };
    let scene = scene_for(dot);
    assert_eq!(ink_ops(&scene).len(), 33);
    assert_eq!(scene.viewbox.min_x, -48.5);
    assert_eq!(scene.viewbox.min_y, -48.5);
    assert_eq!(scene.viewbox.width, 97.0);
    assert_eq!(scene.viewbox.height, 97.0);

    let empty = ExcalidrawElement {
        element_type: "freedraw".into(),
        points: Some(vec![]),
        ..Default::default()
    };
    let scene = scene_for(empty);
    assert!(scene.drawables.is_empty());
    assert_eq!((scene.viewbox.width, scene.viewbox.height), (800.0, 600.0));
}

#[test]
fn rotated_negative_offsets_use_point_pivot_and_fit_viewport() {
    let element = fixture().elements.pop().unwrap();
    let scene = scene_for(element);
    let drawable = &scene.drawables[0];
    assert_eq!(drawable.center, (475.0, 260.0));
    let (sin, cos) = drawable.rotation_deg.to_radians().sin_cos();
    for op in paths(drawable).iter().flat_map(|path| &path.ops) {
        let (x, y) = match *op {
            PathOp::Move(x, y) | PathOp::Line(x, y) => (x, y),
            PathOp::Cubic(x, y, ..) => (x, y),
            PathOp::Close => continue,
        };
        let (dx, dy) = (x - drawable.center.0, y - drawable.center.1);
        let rotated = (
            drawable.center.0 + dx * cos - dy * sin,
            drawable.center.1 + dx * sin + dy * cos,
        );
        assert!(rotated.0 >= scene.viewbox.min_x - 1e-9);
        assert!(rotated.1 >= scene.viewbox.min_y - 1e-9);
        assert!(rotated.0 <= scene.viewbox.min_x + scene.viewbox.width + 1e-9);
        assert!(rotated.1 <= scene.viewbox.min_y + scene.viewbox.height + 1e-9);
    }
}

#[test]
fn svg_and_transparent_png_contain_the_fixture_ink() -> Result<(), Box<dyn Error>> {
    let data = fixture();
    let svg = generate_svg(&data, Some((0, 0, 0, 0)));
    for color in ["#1971c2", "#e03131", "#ffd43b", "#9c36b5", "#f08c00"] {
        assert!(
            svg.contains(&format!("fill=\"{color}\"")),
            "missing {color}"
        );
    }

    let directory = tempfile::tempdir()?;
    let output = directory.path().join("freedraw.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None)?;
    let mut decoder = png::Decoder::new(std::fs::File::open(output)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut bytes)?;
    reader.finish()?;
    for expected in [
        [25, 113, 194],
        [224, 49, 49],
        [255, 212, 59],
        [156, 54, 181],
        [240, 140, 0],
    ] {
        assert!(
            bytes
                .chunks_exact(4)
                .any(|pixel| { pixel[3] == 255 && pixel[..3] == expected }),
            "missing RGB {expected:?}"
        );
    }
    Ok(())
}
