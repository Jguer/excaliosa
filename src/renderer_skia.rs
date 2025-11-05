use crate::arrow_utils::{calc_arrowhead_points, build_elbow_arrow_path, calculate_arrowhead_direction};
use crate::color_utils::{has_fill, has_stroke, parse_color};
use crate::math_utils::{calculate_center, catmull_rom_cubics};
use crate::models::{ExcalidrawData, ExcalidrawElement as Element};
use crate::converter::{EXCALIFONT_REGULAR, LIBERATION_SANS_REGULAR, CASCADIA_CODE};
use crate::rect_utils::{get_corner_radius, generate_rounded_rect_path};
use crate::font_utils::{calculate_text_x_position_for_line, get_font_family};
use crate::stroke_utils::{get_stroke_dash_array, get_dotted_cap_dash_array};
use crate::utils::{calculate_viewbox, save_png_with_quality};
use anyhow::Result;
use euclid::default::Point2D;
use palette::Srgba;
use parley::{FontContext, LayoutContext, StyleProperty};
use rough_tiny_skia::SkiaGenerator;
use roughr::core::{FillStyle, OptionsBuilder};
use skrifa::{GlyphId, MetadataProvider, OutlineGlyph, instance::{LocationRef, NormalizedCoord, Size}, outline::{DrawSettings, OutlinePen}, raw::FontRef as ReadFontsRef};
use tiny_skia::*;



// Type alias for cubic Bezier segment in f32
type CubicBezierSegmentF32 = ((f32, f32), (f32, f32), (f32, f32), (f32, f32));

// Build Catmull–Rom cubic segments in absolute coords
fn catmull_rom_cubics_abs(points: &[(f64, f64)], x: f32, y: f32) -> Vec<CubicBezierSegmentF32> {
    let abs: Vec<(f32, f32)> = points.iter()
        .map(|(px, py)| (x + *px as f32, y + *py as f32))
        .collect();
    catmull_rom_cubics(&abs, 0.5f32)
}

// Compute arrowhead points as per Excalidraw using Catmull-Rom cubics for accurate direction
fn exca_arrowhead_points(
    points: &[(f64,f64)],
    x: f32,
    y: f32,
    stroke_width: f32,
    arrowhead: &str,
    position: &str, // "start" or "end"
) -> Option<Vec<f32>> {
    if points.is_empty() { return None; }
    
    let points_f32: Vec<(f32, f32)> = points.iter()
        .map(|(px, py)| (*px as f32, *py as f32))
        .collect();
    
    // Use shared arrowhead direction calculation
    if let Some((tail_x, tail_y, tip_x, tip_y, seg_len)) = calculate_arrowhead_direction(
        &points_f32,
        x,
        y,
        position,
        0.5f32,
    ) {
        // Use shared arrowhead calculation with Catmull-Rom derived tip/tail
        let pts = calc_arrowhead_points(
            tail_x, tail_y,
            tip_x, tip_y,
            arrowhead,
            stroke_width,
            seg_len
        );
        Some(pts)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_arrowhead_ex(
    pixmap: &mut PixmapMut,
    points: &[(f64,f64)],
    x: f32,
    y: f32,
    stroke_rgba: (u8,u8,u8,u8),
    stroke_width: f32,
    arrowhead: &str,
    position: &str,
    cap_gen: &SkiaGenerator,
) {
    if let Some(vals) = exca_arrowhead_points(points, x, y, stroke_width, arrowhead, position) {
        match arrowhead {
            "dot" | "circle" | "circle_outline" => {
                let cx = vals[0]; let cy = vals[1]; let diameter = vals[2];
                // Render solid/outline circle with tiny-skia
                let mut path_builder = PathBuilder::new();
                path_builder.push_circle(cx, cy, diameter / 2.0);
                if let Some(path) = path_builder.finish() {
                    let mut paint = Paint::default();
                    // Fill color: outline => white background, else stroke color
                    let (fill_r,fill_g,fill_b,fill_a) = if arrowhead == "circle_outline" { (255,255,255,255) } else { stroke_rgba };
                    paint.set_color_rgba8(fill_r,fill_g,fill_b,fill_a);
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    // Stroke outline
                    let mut spaint = Paint::default(); spaint.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let stroke = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &spaint, &stroke, Transform::identity(), None);
                }
            }
            "triangle" | "triangle_outline" => {
                let x0=vals[0]; let y0=vals[1]; let x1=vals[2]; let y1=vals[3]; let x2p=vals[4]; let y2p=vals[5];
                let mut pb = PathBuilder::new(); pb.move_to(x0,y0); pb.line_to(x1,y1); pb.line_to(x2p,y2p); pb.close();
                if let Some(path) = pb.finish() {
                    // Fill
                    let (fr,fg,fb,fa) = if arrowhead.ends_with("_outline") { (255,255,255,255) } else { stroke_rgba };
                    let mut fp = Paint::default(); fp.set_color_rgba8(fr,fg,fb,fa);
                    pixmap.fill_path(&path, &fp, FillRule::Winding, Transform::identity(), None);
                    // Stroke
                    let mut sp = Paint::default(); sp.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let st = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &sp, &st, Transform::identity(), None);
                }
            }
            "diamond" | "diamond_outline" => {
                let x0=vals[0]; let y0=vals[1]; let x1=vals[2]; let y1=vals[3]; let ox=vals[4]; let oy=vals[5]; let x2p=vals[6]; let y2p=vals[7];
                let mut pb = PathBuilder::new(); pb.move_to(x0,y0); pb.line_to(x1,y1); pb.line_to(ox,oy); pb.line_to(x2p,y2p); pb.close();
                if let Some(path) = pb.finish() {
                    let (fr,fg,fb,fa) = if arrowhead.ends_with("_outline") { (255,255,255,255) } else { stroke_rgba };
                    let mut fp = Paint::default(); fp.set_color_rgba8(fr,fg,fb,fa);
                    pixmap.fill_path(&path, &fp, FillRule::Winding, Transform::identity(), None);
                    let mut sp = Paint::default(); sp.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let st = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &sp, &st, Transform::identity(), None);
                }
            }
            "crowfoot_one" => {
                // vals: [x2,y2,x3,y3,x4,y4] per getArrowheadPoints, but here we use x3,y3,x4,y4 line
                let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let line = cap_gen.line::<f32>(x3,y3,x4,y4);
                line.draw(pixmap);
            }
            "bar" => {
                // Draw only the perpendicular bar line x3,y3 -> x4,y4
                let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let line = cap_gen.line::<f32>(x3,y3,x4,y4);
                line.draw(pixmap);
            }
            _ => {
                // default/arrow/bar/crowfoot_many/one_or_many -> two lines to tip
                let x2=vals[0]; let y2=vals[1]; let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let l1 = cap_gen.line::<f32>(x3,y3,x2,y2); l1.draw(pixmap);
                let l2 = cap_gen.line::<f32>(x4,y4,x2,y2); l2.draw(pixmap);
                // extra for crowfoot_one_or_many: add crowfoot_one bar
                if arrowhead == "crowfoot_one_or_many" {
                    if let Some(bar) = exca_arrowhead_points(points,x,y,stroke_width,"crowfoot_one",position) {
                        let bx3=bar[2]; let by3=bar[3]; let bx4=bar[4]; let by4=bar[5];
                        let bl = cap_gen.line::<f32>(bx3,by3,bx4,by4); bl.draw(pixmap);
                    }
                }
            }
        }
    }
}


/// Build Catmull–Rom cubic path d-string (M + C segments)
fn build_catmull_rom_cubic_path(points: &[(f64,f64)], x:f32, y:f32) -> Option<String> {
    let segs = catmull_rom_cubics_abs(points, x, y);
    if segs.is_empty() { return None; }
    let mut d = String::new();
    d.push_str(&format!("M {} {}", segs[0].0 .0, segs[0].0 .1));
    for (_, c1, c2, p3) in segs {
        d.push_str(&format!(" C {} {}, {} {}, {} {}", c1.0, c1.1, c2.0, c2.1, p3.0, p3.1));
    }
    Some(d)
}

/// Helper struct for rendering glyphs with tiny-skia (implements OutlinePen)
struct TinySkiaPen<'a> {
    pixmap: &'a mut PixmapMut<'a>,
    x: f32,
    y: f32,
    paint: Paint<'static>,
    open_path: PathBuilder,
}

impl<'a> TinySkiaPen<'a> {
    fn new(pixmap: &'a mut PixmapMut<'a>) -> TinySkiaPen<'a> {
        TinySkiaPen {
            pixmap,
            x: 0.0,
            y: 0.0,
            paint: Paint::default(),
            open_path: PathBuilder::new(),
        }
    }

    fn set_origin(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn set_color(&mut self, color: Color) {
        self.paint.set_color(color);
    }

    fn draw_glyph(
        &mut self,
        glyph: &OutlineGlyph<'_>,
        font_size: f32,
        normalized_coords: &[NormalizedCoord],
    ) {
        let settings = DrawSettings::unhinted(Size::new(font_size), LocationRef::new(normalized_coords));
        glyph.draw(settings, self).ok();
    }

    fn finish_path(&mut self) {
        let builder = std::mem::replace(&mut self.open_path, PathBuilder::new());
        if let Some(path) = builder.finish() {
            self.pixmap.fill_path(
                &path,
                &self.paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

impl OutlinePen for TinySkiaPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.open_path.move_to(self.x + x, self.y - y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.open_path.line_to(self.x + x, self.y - y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.open_path
            .quad_to(self.x + cx0, self.y - cy0, self.x + x, self.y - y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.open_path.cubic_to(
            self.x + cx0,
            self.y - cy0,
            self.x + cx1,
            self.y - cy1,
            self.x + x,
            self.y - y,
        );
    }

    fn close(&mut self) {
        self.open_path.close();
    }
}

/// Context for text rendering operations
struct TextRenderContext<'a> {
    font_cx: &'a mut FontContext,
    layout_cx: &'a mut LayoutContext,
    custom_fonts: &'a std::collections::HashMap<String, Vec<u8>>,
}

/// Properties for rendering text
struct TextProperties<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_family: &'static str,
    text_align: Option<&'a str>,
    container_width: f32,
}

/// Render text using Parley and tiny-skia
fn render_text<'a>(
    pixmap: &'a mut PixmapMut<'a>,
    props: &TextProperties<'a>,
    text_ctx: &mut TextRenderContext<'a>,
) {
    // Skip empty text
    if props.text.is_empty() {
        return;
    }
    
    // Check if we have a custom font for this family
    if let Some(font_data) = text_ctx.custom_fonts.get(props.font_family) {
        // Use skrifa to render directly with our custom font
        if let Ok(font_ref) = ReadFontsRef::new(font_data.as_slice()) {
            render_text_with_skrifa(
                pixmap, 
                props.text, 
                props.x, 
                props.y, 
                props.font_size, 
                props.color, 
                &font_ref, 
                props.text_align, 
                props.container_width
            );
            return;
        }
    }
    
    let display_scale = 1.0;
    
    // Create a layout builder with parley (fallback to system fonts)
    let mut builder = text_ctx.layout_cx.ranged_builder(text_ctx.font_cx, props.text, display_scale, false);
    
    // Set font properties with the specified font family
    builder.push_default(StyleProperty::FontStack(parley::style::FontStack::Source(props.font_family.into())));
    builder.push_default(StyleProperty::FontSize(props.font_size));
    
    // Build the layout
    let mut layout = builder.build(props.text);
    layout.break_all_lines(None);
    
    // Create pen for rendering
    let mut pen = TinySkiaPen::new(pixmap);
    let text_color = Color::from_rgba8(props.color.0, props.color.1, props.color.2, props.color.3);
    
    // Render each glyph run
    for line in layout.lines() {
        for item in line.items() {
            if let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let mut run_x = glyph_run.offset();
                let run_y = glyph_run.baseline();
                
                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let normalized_coords = run
                    .normalized_coords()
                    .iter()
                    .map(|coord| NormalizedCoord::from_bits(*coord))
                    .collect::<Vec<_>>();
                
                // Get font outlines
                let font_collection_ref = font.data.as_ref();
                if let Ok(font_ref) = ReadFontsRef::from_index(font_collection_ref, font.index) {
                    let outlines = font_ref.outline_glyphs();
                    
                    // Render each glyph
                    for glyph in glyph_run.glyphs() {
                        let glyph_x = props.x + run_x + glyph.x;
                        let glyph_y = props.y + run_y - glyph.y;
                        run_x += glyph.advance;
                        
                        let glyph_id = GlyphId::from(glyph.id);
                        if let Some(glyph_outline) = outlines.get(glyph_id) {
                            pen.set_origin(glyph_x, glyph_y);
                            pen.set_color(text_color);
                            pen.draw_glyph(&glyph_outline, font_size, &normalized_coords);
                            pen.finish_path();
                        }
                    }
                }
            }
        }
    }
}

/// Render text directly using skrifa without parley
#[allow(clippy::too_many_arguments)]
fn render_text_with_skrifa<'a>(
    pixmap: &'a mut PixmapMut<'a>,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_ref: &ReadFontsRef,
    text_align: Option<&str>,
    container_width: f32,
) {
    let mut pen = TinySkiaPen::new(pixmap);
    let text_color = Color::from_rgba8(color.0, color.1, color.2, color.3);
    
    let outlines = font_ref.outline_glyphs();
    let charmap = font_ref.charmap();
    let glyph_metrics = font_ref.glyph_metrics(Size::new(font_size), LocationRef::default());
    
    // Get font metrics for line height calculation
    let metrics = font_ref.metrics(Size::new(font_size), LocationRef::default());
    let line_height = (metrics.ascent - metrics.descent + metrics.leading) * 1.25; // 1.25 is typical line height multiplier
    
    let mut cursor_y = y;
    
    // Split text by newlines and render each line
    let lines: Vec<&str> = text.split('\n').collect();
    
    for (line_idx, line) in lines.iter().enumerate() {
        // Calculate line width for alignment
        let mut line_width = 0.0f32;
        for ch in line.chars() {
            if let Some(glyph_id) = charmap.map(ch) {
                if let Some(advance) = glyph_metrics.advance_width(glyph_id) {
                    line_width += advance;
                }
            }
        }
        
        let start_x = calculate_text_x_position_for_line(x, container_width, line_width, text_align);
        let mut cursor_x = start_x;
        
        // Render each character in the line
        for ch in line.chars() {
            if let Some(glyph_id) = charmap.map(ch) {
                if let Some(glyph_outline) = outlines.get(glyph_id) {
                    pen.set_origin(cursor_x, cursor_y);
                    pen.set_color(text_color);
                    pen.draw_glyph(&glyph_outline, font_size, &[]);
                    pen.finish_path();
                }
                // Advance cursor horizontally
                if let Some(advance) = glyph_metrics.advance_width(glyph_id) {
                    cursor_x += advance;
                }
            }
        }
        
        // Move to next line if not the last line
        if line_idx < lines.len() - 1 {
            cursor_y += line_height;
        }
    }
}

fn render_element<'a, 'b: 'a>(
    pixmap: &'a mut PixmapMut<'a>, 
    element: &'b Element,
    offset: (f32, f32),
    text_ctx: &mut TextRenderContext<'a>,
    transform: Transform,
) {
    if element.is_deleted {
        return;
    }
    
    // Extract scale factor from transform (sx for uniform scaling)
    let scale = transform.sx;
    
    // Transform coordinates relative to viewbox and apply scale
    let x = ((element.x - offset.0 as f64) * scale as f64) as f32;
    let y = ((element.y - offset.1 as f64) * scale as f64) as f32;
    let width = (element.width * scale as f64) as f32;
    let height = (element.height * scale as f64) as f32;

    let stroke_rgba = parse_color(&element.stroke_color);
    let fill_rgba = parse_color(&element.background_color);

    let should_stroke = has_stroke(element);
    let should_fill = has_fill(element);

    // Map Excalidraw fill styles to roughr FillStyle
    let fill_style = match element.fill_style.as_str() {
        "hachure" => FillStyle::Hachure,
        "cross-hatch" => FillStyle::CrossHatch,
        "solid" => FillStyle::Solid,
        _ => FillStyle::Hachure,
    };

    // Create rough options
    let mut options_builder = OptionsBuilder::default();
    
    if should_stroke {
        options_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        options_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    
    if should_fill {
        options_builder.fill(
            Srgba::from_components((fill_rgba.0, fill_rgba.1, fill_rgba.2, fill_rgba.3))
                .into_format(),
        );
        options_builder.fill_style(fill_style);
    }

    // Set roughness
    options_builder.roughness(element.roughness as f32);
    options_builder.seed(element.seed as u64);
    // Stroke dash pattern per strokeStyle (use scaled stroke width)
    let scaled_stroke_width = (element.stroke_width * scale as f64) as f32;
    if let Some(dash) = get_stroke_dash_array(&element.stroke_style, scaled_stroke_width as f64) {
        // Prefer backend dash support if exposed by roughr
        #[allow(unused_must_use)]
        {
            options_builder.stroke_line_dash(dash);
        }
    }
    
    // DPI for fill weight (scale with transform)
    const BASE_DPI: f32 = 96.0;
    let dpi = BASE_DPI * scale;
    options_builder.fill_weight(dpi * 0.01);

    let options = options_builder.build().unwrap();
    let generator = SkiaGenerator::new(options);

    // Helper: stroke-only generator for linear/arrow paths to avoid fill paths
    let mut stroke_only_builder = OptionsBuilder::default();
    if should_stroke {
        stroke_only_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        stroke_only_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    stroke_only_builder.roughness(element.roughness as f32);
    stroke_only_builder.seed(element.seed as u64);
    stroke_only_builder.fill_weight(dpi * 0.01);
    if let Some(dash) = get_stroke_dash_array(&element.stroke_style, scaled_stroke_width as f64) {
        #[allow(unused_must_use)]
        {
            stroke_only_builder.stroke_line_dash(dash);
        }
    }
    let stroke_only_options = stroke_only_builder.build().unwrap();
    let stroke_gen = SkiaGenerator::new(stroke_only_options);

    // Build a separate generator for arrowhead caps:
    // - solid for solid/dashed
    // - dotted with reduced gap for dotted
    let mut cap_builder = OptionsBuilder::default();
    if should_stroke {
        cap_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        cap_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    cap_builder.roughness(element.roughness as f32);
    cap_builder.seed(element.seed as u64);
    cap_builder.fill_weight(dpi * 0.01);
    if element.stroke_style == "dotted" {
        #[allow(unused_must_use)]
        {
            let dash = get_dotted_cap_dash_array(scaled_stroke_width as f64);
            cap_builder.stroke_line_dash(dash);
        }
    }
    let cap_gen = SkiaGenerator::new(cap_builder.build().unwrap());

    // Render based on element type
    match element.element_type.as_str() {
        "rectangle" => {
            // Check if rectangle has roundness
            if element.roundness.is_some() {
                // Convert f32 to f64 for corner radius calculation, then use shared path generation
                let r = get_corner_radius((width.min(height)) as f64, element) as f32;
                // Use shared rounded rectangle path generation (uses quadratic curves)
                let path_d = generate_rounded_rect_path(x as f64, y as f64, width as f64, height as f64, r as f64);

                let rounded_rect = generator.path::<f32>(path_d);
                rounded_rect.draw(pixmap);
            } else {
                let rect = generator.rectangle::<f32>(x, y, width, height);
                rect.draw(pixmap);
            }
        }
        "ellipse" => {
            // rough.js ellipse expects center coordinates (cx, cy) and diameters (width, height)
            let (cx, cy) = calculate_center(x, y, width, height);
            let ellipse = generator.ellipse::<f32>(cx, cy, width, height);
            ellipse.draw(pixmap);
        }
        "diamond" => {
            // Create diamond path using polygon
            let (cx, cy) = calculate_center(x, y, width, height);
            let points = vec![
                Point2D::new(cx, y),           // top
                Point2D::new(x + width, cy),   // right
                Point2D::new(cx, y + height),  // bottom
                Point2D::new(x, cy),           // left
            ];
            let polygon = generator.polygon(&points);
            polygon.draw(pixmap);
        }
        "line" => {
            if let Some(ref points) = element.points {
                if points.len() >= 2 {
                    // Scale points coordinates
                    let scaled_points: Vec<(f64, f64)> = points.iter()
                        .map(|p| (p.0 * scale as f64, p.1 * scale as f64))
                        .collect();
                    if element.roundness.is_some() {
                        if let Some(path_d) = build_catmull_rom_cubic_path(&scaled_points, x, y) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else {
                        // straight polyline via rough path M/L
                        let mut d = String::new();
                        d.push_str(&format!("M {} {}", x + scaled_points[0].0 as f32, y + scaled_points[0].1 as f32));
                        for p in scaled_points.iter().skip(1) {
                            d.push_str(&format!(" L {} {}", x + p.0 as f32, y + p.1 as f32));
                        }
                        let path = stroke_gen.path::<f32>(d);
                        path.draw(pixmap);
                    }
                }
            }
        }
        "arrow" => {
            if let Some(ref points) = element.points {
                if points.len() >= 2 {
                    // Scale points coordinates
                    let scaled_points: Vec<(f64, f64)> = points.iter()
                        .map(|p| (p.0 * scale as f64, p.1 * scale as f64))
                        .collect();
                    
                    // Draw curve or polyline depending on style
                    if element.elbowed.unwrap_or(false) && scaled_points.len() >= 3 {
                        // Convert relative points to absolute
                        let abs_points: Vec<(f64, f64)> = scaled_points.iter()
                            .map(|p| (x as f64 + p.0, y as f64 + p.1))
                            .collect();
                        
                        if let Some(path_d) = build_elbow_arrow_path(&abs_points, 16.0 * scale as f64) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else if element.roundness.is_some() && scaled_points.len() >= 2 {
                        if let Some(path_d) = build_catmull_rom_cubic_path(&scaled_points, x, y) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else {
                        // straight polyline via rough path M/L
                        let mut d = String::new();
                        d.push_str(&format!("M {} {}", x + scaled_points[0].0 as f32, y + scaled_points[0].1 as f32));
                        for p in scaled_points.iter().skip(1) {
                            d.push_str(&format!(" L {} {}", x + p.0 as f32, y + p.1 as f32));
                        }
                        let path = stroke_gen.path::<f32>(d);
                        path.draw(pixmap);
                    }

                    // Draw start arrowhead if specified
                    if let Some(ref start_arrowhead) = element.start_arrowhead {
                        draw_arrowhead_ex(
                            pixmap,
                            &scaled_points,
                            x,
                            y,
                            stroke_rgba,
                            scaled_stroke_width,
                            start_arrowhead,
                            "start",
                            &cap_gen,
                        );
                    }
                    
                    // Draw end arrowhead if specified
                    if let Some(ref end_arrowhead) = element.end_arrowhead {
                        draw_arrowhead_ex(
                            pixmap,
                            &scaled_points,
                            x,
                            y,
                            stroke_rgba,
                            scaled_stroke_width,
                            end_arrowhead,
                            "end",
                            &cap_gen,
                        );
                    }
                }
            }
        }
        "text" => {
            // Render text element
            if let Some(ref text) = element.text {
                let font_size = (element.font_size.unwrap_or(20.0) * scale as f64) as f32;
                let font_family = get_font_family(element.font_family);
                // Create TextProperties with lifetimes tied to element
                let text_props = TextProperties {
                    text: text.as_str(),
                    x,
                    y: y + font_size,
                    font_size,
                    color: stroke_rgba,
                    font_family,
                    text_align: element.text_align.as_deref(),
                    container_width: width,
                };
                // Render text - the lifetime is satisfied because text_props only lives for this scope
                render_text(pixmap, &text_props, text_ctx);
            }
        }
        _ => {
            // Unsupported element type
            eprintln!("Unsupported element type: {}", element.element_type);
        }
    }
}

/// Load custom fonts from embedded bytes
fn load_custom_fonts() -> std::collections::HashMap<String, Vec<u8>> {
    let mut fonts = std::collections::HashMap::new();
    
    // Load fonts from embedded bytes
    fonts.insert("Liberation Sans".to_string(), LIBERATION_SANS_REGULAR.to_vec());
    fonts.insert("Cascadia Code".to_string(), CASCADIA_CODE.to_vec());
    fonts.insert("Excalifont".to_string(), EXCALIFONT_REGULAR.to_vec());
    
    eprintln!("Loaded {} custom fonts from embedded bytes", fonts.len());
    fonts
}


pub fn render_to_png(
    data: &ExcalidrawData,
    output_path: &std::path::Path,
    background: Option<(u8, u8, u8, u8)>,
    quality: u8,
    dpi: Option<u32>,
) -> Result<()> {
    let viewbox = calculate_viewbox(&data.elements);
    
    // Calculate scale factor from DPI (assume source is 96 DPI)
    const SOURCE_DPI: f32 = 96.0;
    let scale = dpi.map(|d| d as f32 / SOURCE_DPI).unwrap_or(1.0);
    
    let width = (viewbox.width * scale as f64).ceil() as u32;
    let height = (viewbox.height * scale as f64).ceil() as u32;
    
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

    // Fill background if provided (or default to white if None)
    if let Some((r, g, b, a)) = background.or(Some((255, 255, 255, 255))) {
        if a > 0 {
            let mut background_paint = Paint::default();
            background_paint.set_color_rgba8(r, g, b, a);
            pixmap.fill_rect(
                Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &background_paint,
                Transform::identity(),
                None,
            );
        }
    }
    
    // Create font and layout contexts for text rendering
    let mut font_cx = FontContext::default();
    let mut layout_cx = LayoutContext::new();
    
    // Load custom fonts from the fonts directory
    let custom_fonts = load_custom_fonts();
    
    // Create transform matrix for scaling
    let transform = Transform::from_scale(scale, scale);
    
    // Render each element
    for element in &data.elements {
        // Create text rendering context for each element to avoid borrowing conflicts
        let mut text_ctx = TextRenderContext {
            font_cx: &mut font_cx,
            layout_cx: &mut layout_cx,
            custom_fonts: &custom_fonts,
        };
        
        render_element(
            &mut pixmap.as_mut(),
            element,
            (viewbox.min_x as f32, viewbox.min_y as f32),
            &mut text_ctx,
            transform,
        );
    }
    
    // Save to PNG with quality control
    save_png_with_quality(&pixmap, output_path, quality)?;
    
    Ok(())
}
