use anyhow::Result;
use excaliosa::color_utils::{parse_color, parse_color_result};
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use std::process::Command;

const FIXTURE: &str = include_str!("fixtures/named_element_colors.json");

fn fixture(hex_equivalent: bool) -> ExcalidrawData {
    let json = if hex_equivalent {
        FIXTURE
            .replace(" DaRkSlaTeBlue ", "#483d8b")
            .replace(" ReBeCCaPurple ", "#663399")
            .replace("aQuA", "#00ffff")
            .replace(" GrEy ", "#808080")
            .replace(" TransParent ", "#00000000")
            .replace(" RebeccaPurple ", "#663399")
            .replace(" ToMaTo ", "#ff6347")
            .replace(" LiGhTbLue ", "#add8e6")
    } else {
        FIXTURE.to_owned()
    };
    serde_json::from_str(&json).unwrap()
}

fn decoded_png(data: &ExcalidrawData) -> Result<(u32, u32, Vec<u8>)> {
    let dir = tempfile::tempdir()?;
    let output = dir.path().join("named-colors.png");
    render_to_png(data, &output, None, 75, None)?;
    let mut decoder = png::Decoder::new(std::fs::File::open(output)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    reader.finish()?;
    Ok((info.width, info.height, pixels))
}

#[test]
fn scene_parser_accepts_names_aliases_and_existing_transparency() {
    for (input, expected) in [
        (" ReBeCCaPurple ", (102, 51, 153, 255)),
        ("aQuA", (0, 255, 255, 255)),
        ("CYAN", (0, 255, 255, 255)),
        ("grey", (128, 128, 128, 255)),
        ("GrAy", (128, 128, 128, 255)),
        (" TransParent ", (0, 0, 0, 0)),
        ("#Ab3", (170, 187, 51, 255)),
        ("#aabbcc00", (170, 187, 204, 0)),
    ] {
        assert_eq!(parse_color(input), expected, "{input}");
    }
    assert_eq!(parse_color("not-a-color"), (0, 0, 0, 255));
    assert_eq!(parse_color(""), (0, 0, 0, 0));
    assert!(parse_color_result("red").is_err());
}

#[test]
fn named_svg_paints_match_hex_and_saved_color_only_fills_caps() {
    let named = fixture(false);
    let svg = generate_svg(&named, None);
    assert_eq!(svg, generate_svg(&fixture(true), None));
    assert!(svg.contains("fill=\"#663399\" stroke=\"none\""));
    assert!(svg.contains("fill=\"none\" stroke=\"#483d8b\""));
    assert!(svg.contains("fill=\"#808080\" stroke=\"none\""));
    assert!(svg.contains("fill=\"none\" stroke=\"#00ffff\""));
    assert!(svg.contains("fill=\"none\" stroke=\"#000000\""));
    assert!(svg.contains("font-family=\"Liberation Sans, sans-serif\" fill=\"#663399\""));
    assert_eq!(svg.matches("fill=\"#add8e6\"").count(), 1);
    assert!(build_scene(&named, None).background.is_none());

    let transparent = Some((0, 0, 0, 0));
    assert_eq!(
        generate_svg(&named, transparent),
        generate_svg(&fixture(true), transparent)
    );
    assert!(!generate_svg(&named, transparent).contains("fill=\"#add8e6\""));
}

#[test]
fn named_png_pixels_match_hex_and_keep_default_page_background() -> Result<()> {
    let named = decoded_png(&fixture(false))?;
    let hex = decoded_png(&fixture(true))?;
    assert_eq!(named, hex);
    let (_, _, pixels) = named;
    assert!(pixels.chunks(4).any(|pixel| pixel == [255, 255, 255, 255]));
    assert!(pixels.chunks(4).any(|pixel| pixel == [173, 216, 230, 255]));
    Ok(())
}

#[test]
fn cli_still_rejects_named_backgrounds() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let output_path = dir.path().join("rejected.svg");
    let output = Command::new(env!("CARGO_BIN_EXE_excaliosa"))
        .arg("tests/fixtures/named_element_colors.json")
        .args(["--background", "red", "-o"])
        .arg(&output_path)
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid --background value"));
    assert!(!output_path.exists());
    Ok(())
}
