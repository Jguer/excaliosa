use excaliosa::models::ViewBox;
use excaliosa::scene::Primitive;
use excaliosa::{ExcalidrawData, ExcalidrawElement, build_scene, generate_svg, render_to_png};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/frames.json")).unwrap()
}

fn wrap(elements: Vec<ExcalidrawElement>) -> ExcalidrawData {
    ExcalidrawData {
        data_type: "excalidraw".to_owned(),
        version: None,
        version_nonce: None,
        source: None,
        elements,
        app_state: Default::default(),
        files: Default::default(),
    }
}

#[test]
fn frame_names_outlines_clips_and_opacity_stay_in_file_order() {
    let scene = build_scene(&fixture(), None);
    assert_eq!(scene.drawables.len(), 17);

    let Primitive::Text(default_name) = &scene.drawables[0].primitive else {
        panic!("frame name must precede its outline");
    };
    assert_eq!(default_name.content, "Frame");
    assert_eq!(default_name.font_family, "Liberation Sans");
    assert_eq!(default_name.font_size, 14.0);
    assert!(scene.drawables[0].frame_clip.is_none());

    let Primitive::Paths(outline) = &scene.drawables[1].primitive else {
        panic!("frame outline must follow its name");
    };
    assert_eq!(outline[0].stroke_width, 2.0);
    assert!(outline[0].fill.is_none());

    let Primitive::Text(truncated) = &scene.drawables[5].primitive else {
        panic!("magic frame name must precede its outline");
    };
    assert!(truncated.content.ends_with("..."));
    assert!(truncated.content.len() < "A magic frame with a deliberately very long name".len());

    for index in [2, 3, 4, 7, 14, 15, 16] {
        assert!(scene.drawables[index].frame_clip.is_some(), "index {index}");
    }
    for index in [8, 9, 10, 11] {
        assert!(scene.drawables[index].frame_clip.is_none(), "index {index}");
    }
    assert!((scene.drawables[2].opacity - 0.4).abs() < 1e-9);
    assert!((scene.drawables[8].opacity - 0.5).abs() < 1e-9);
    assert!((scene.drawables[14].opacity - 0.8).abs() < 1e-9);
    assert_eq!(scene.drawables[10].opacity, 1.0);

    let rotated_clip = scene.drawables[14].frame_clip.as_ref().unwrap();
    assert_ne!(rotated_clip.rotation_deg, 0.0);
    assert_eq!(rotated_clip.center, (160.0, 450.0));
    assert!(scene.viewbox.min_y <= 100.0 - 40.0 - 3.0 - 17.5);
}

#[test]
fn missing_deleted_and_non_frame_parents_do_not_clip_or_fade() {
    let live_frame = ExcalidrawElement {
        id: "live".to_owned(),
        element_type: "frame".to_owned(),
        width: 100.0,
        height: 100.0,
        opacity: 40.0,
        ..Default::default()
    };
    for parent in [
        ExcalidrawElement {
            is_deleted: true,
            ..live_frame.clone()
        },
        ExcalidrawElement {
            element_type: "rectangle".to_owned(),
            ..live_frame.clone()
        },
    ] {
        let child = ExcalidrawElement {
            element_type: "rectangle".to_owned(),
            frame_id: Some("live".to_owned()),
            width: 20.0,
            height: 20.0,
            ..Default::default()
        };
        let scene = build_scene(&wrap(vec![parent, child]), None);
        let drawable = scene.drawables.last().unwrap();
        assert!(drawable.frame_clip.is_none());
        assert_eq!(drawable.opacity, 1.0);
    }

    let magic = ExcalidrawElement {
        element_type: "magicframe".to_owned(),
        width: 100.0,
        height: 100.0,
        ..Default::default()
    };
    let scene = build_scene(&wrap(vec![magic]), None);
    let Primitive::Text(name) = &scene.drawables[0].primitive else {
        panic!("missing magic frame name");
    };
    assert_eq!(name.content, "AI Frame");
}

#[test]
fn deleted_arrows_do_not_exempt_their_stale_text_from_clipping() {
    let frame = ExcalidrawElement {
        id: "frame".to_owned(),
        element_type: "frame".to_owned(),
        width: 50.0,
        height: 50.0,
        ..Default::default()
    };
    let arrow = ExcalidrawElement {
        id: "deleted-arrow".to_owned(),
        element_type: "arrow".to_owned(),
        is_deleted: true,
        ..Default::default()
    };
    let text = ExcalidrawElement {
        element_type: "text".to_owned(),
        text: Some("outside".to_owned()),
        x: 60.0,
        width: 50.0,
        height: 20.0,
        frame_id: Some("frame".to_owned()),
        container_id: Some("deleted-arrow".to_owned()),
        ..Default::default()
    };
    let scene = build_scene(&wrap(vec![frame, arrow, text]), None);
    assert!(scene.drawables.last().unwrap().frame_clip.is_some());
}

#[test]
fn frame_names_are_xml_safe_and_truncated_in_linear_time() {
    let data: ExcalidrawData = serde_json::from_str(
        r#"{"type":"excalidraw","elements":[{"type":"frame","name":"A\u0000B","width":100,"height":100}]}"#,
    )
    .unwrap();
    let svg = generate_svg(&data, None);
    assert!(!svg.contains('\0'));
    assert!(svg.contains(">A�B</tspan>"));

    let long = ExcalidrawElement {
        element_type: "frame".to_owned(),
        name: Some("A".repeat(100_000)),
        width: 100.0,
        height: 100.0,
        ..Default::default()
    };
    let scene = build_scene(&wrap(vec![long]), None);
    let Primitive::Text(title) = &scene.drawables[0].primitive else {
        panic!("long frame name should be truncated, not omitted");
    };
    assert!(title.content.ends_with("..."));
    assert!(title.content.len() < 100);
}

#[test]
fn frame_title_bounds_match_multiline_and_empty_titles() {
    let multiline = ExcalidrawElement {
        element_type: "frame".to_owned(),
        name: Some("Top\nBottom".to_owned()),
        y: 100.0,
        width: 100.0,
        height: 100.0,
        ..Default::default()
    };
    let scene = build_scene(&wrap(vec![multiline]), None);
    let Primitive::Text(title) = &scene.drawables[0].primitive else {
        panic!("multiline frame title missing");
    };
    assert_eq!(title.y, 62.0);
    assert_eq!(scene.viewbox.min_y, 22.0);

    let empty = ExcalidrawElement {
        element_type: "frame".to_owned(),
        name: Some(String::new()),
        width: 4_016.0,
        height: 4_016.0,
        ..Default::default()
    };
    let data = wrap(vec![empty]);
    let scene = build_scene(&data, None);
    assert_eq!(
        (scene.viewbox.width, scene.viewbox.height),
        (4_096.0, 4_096.0)
    );
    std::fs::create_dir_all(".pi-cache").unwrap();
    let dir = tempfile::tempdir_in(".pi-cache").unwrap();
    let output = dir.path().join("empty-frame-name.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();
    let file = std::fs::File::open(output).unwrap();
    assert_eq!(
        png::Decoder::new(file).read_header_info().unwrap().size(),
        (4_096, 4_096)
    );
}

#[test]
fn svg_uses_unique_user_space_clips_outside_element_transforms() {
    let svg = generate_svg(&fixture(), Some((0, 0, 0, 0)));
    assert_eq!(svg.matches("<clipPath").count(), 7);
    assert_eq!(svg.matches("clipPathUnits=\"userSpaceOnUse\"").count(), 7);
    assert_eq!(
        svg.matches("clip-path=\"url(#excaliosa-frame-clip-")
            .count(),
        7
    );
    assert_eq!(svg.matches("stroke=\"#bbbbbb\"").count(), 3);
    assert!(svg.contains(">Frame</tspan>"));
    assert!(svg.contains("opacity=\"0.5000\""));
    assert!(svg.contains("opacity=\"0.8000\""));
    assert!(svg.contains("fill-opacity=\"0.5000\""));
    assert!(svg.contains("stroke-opacity=\"0.5000\""));
    assert!(svg.contains("excaliosa-frame-clip-14"));
    let rotation = format!("rotate({} 160 450)", 0.35_f64.to_degrees());
    assert!(svg.contains(&rotation));
}

struct Raster {
    width: usize,
    pixels: Vec<u8>,
}

impl Raster {
    fn pixel(&self, view: ViewBox, x: f64, y: f64) -> [u8; 4] {
        let px = (x - view.min_x).floor() as usize;
        let py = (y - view.min_y).floor() as usize;
        let offset = (py * self.width + px) * 4;
        self.pixels[offset..offset + 4].try_into().unwrap()
    }
}

fn render_fixture() -> (Raster, ViewBox) {
    std::fs::create_dir_all(".pi-cache").unwrap();
    let dir = tempfile::tempdir_in(".pi-cache").unwrap();
    let output = dir.path().join("frames.png");
    let data = fixture();
    let view = build_scene(&data, None).viewbox;
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();

    let decoder = png::Decoder::new(std::fs::File::open(output).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    (
        Raster {
            width: info.width as usize,
            pixels,
        },
        view,
    )
}

#[test]
fn transparent_png_clips_rounded_rotated_children_and_compounds_opacity() {
    let (raster, view) = render_fixture();

    let red = raster.pixel(view, 100.0, 180.0);
    assert!(red[0] > 200 && red[1] < 20 && red[2] < 20);
    assert_eq!(red[3], 102);

    assert_eq!(raster.pixel(view, 280.0, 180.0)[3], 128);
    assert_eq!(raster.pixel(view, 320.0, 180.0)[3], 0);
    assert_eq!(raster.pixel(view, 40.5, 100.5)[3], 0);

    let blue = raster.pixel(view, 650.0, 170.0);
    assert!(blue[2] > 200 && blue[0] < 80 && blue[3] == 255);
    assert_eq!(raster.pixel(view, 700.0, 170.0)[3], 0);
    assert!(raster.pixel(view, 275.0, 135.0)[3] > 0);
    assert_eq!(raster.pixel(view, 315.0, 135.0)[3], 0);
    assert_eq!(raster.pixel(view, 115.0, 245.0)[3], 128);
    assert_eq!(raster.pixel(view, 115.0, 232.0)[3], 192);
    assert_eq!(raster.pixel(view, 350.0, 350.0)[3], 255);
    assert_eq!(raster.pixel(view, 600.0, 420.0)[3], 255);

    let arrow = raster.pixel(view, 350.0, 294.0);
    assert!(arrow[3] > 0 && arrow[0] < 80 && arrow[1] < 80 && arrow[2] < 80);

    let rotate = |x: f64, y: f64| {
        let (sin, cos) = 0.35_f64.sin_cos();
        let (dx, dy) = (x - 160.0, y - 450.0);
        (160.0 + dx * cos - dy * sin, 450.0 + dx * sin + dy * cos)
    };
    let corner = rotate(70.5, 400.5);
    assert_eq!(raster.pixel(view, corner.0, corner.1)[3], 0);
    let interior = rotate(100.0, 420.0);
    let purple = raster.pixel(view, interior.0, interior.1);
    assert!(purple[0] > 80 && purple[2] > 150 && purple[3] == 204);
}
