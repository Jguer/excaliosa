/// Mathematical utilities for curve generation and point calculations
/// Type alias for cubic Bezier segment: (start_point, control_point_1, control_point_2, end_point)
pub type CubicBezierSegment<T> = ((T, T), (T, T), (T, T), (T, T));

/// Build Catmull-Rom cubic Bezier segments from points
/// Returns a vector of (p0, cp1, cp2, p1) tuples representing cubic Bezier curves
/// 
/// # Arguments
/// * `points` - Points in absolute coordinates
/// * `tension` - Catmull-Rom tension parameter (typically 0.5)
pub fn catmull_rom_cubics<T>(points: &[(T, T)], tension: T) -> Vec<CubicBezierSegment<T>>
where
    T: num_traits::Float + Copy,
{
    if points.len() < 2 {
        return vec![];
    }
    if points.len() == 2 {
        return vec![(points[0], points[0], points[1], points[1])];
    }
    
    let mut segs = Vec::new();
    
    // Helper to get point with endpoint duplication (Catmull-Rom style)
    let get = |i: isize| -> (T, T) {
        let n = points.len() as isize;
        let idx = if i < 0 { 0 } else if i >= n { n - 1 } else { i } as usize;
        points[idx]
    };
    
    for i in 0..(points.len() - 1) {
        let p0 = get(i as isize - 1);
        let p1 = get(i as isize);
        let p2 = get(i as isize + 1);
        let p3 = get(i as isize + 2);
        
        // Catmull-Rom to cubic Bezier control points
        let tangent1_x = (p2.0 - p0.0) * tension;
        let tangent1_y = (p2.1 - p0.1) * tension;
        let tangent2_x = (p3.0 - p1.0) * tension;
        let tangent2_y = (p3.1 - p1.1) * tension;
        
        let cp1 = (p1.0 + tangent1_x / T::from(3.0).unwrap(), p1.1 + tangent1_y / T::from(3.0).unwrap());
        let cp2 = (p2.0 - tangent2_x / T::from(3.0).unwrap(), p2.1 - tangent2_y / T::from(3.0).unwrap());
        
        segs.push((p1, cp1, cp2, p2));
    }
    
    segs
}

/// Calculate distance between two points
pub fn distance<T>(p1: (T, T), p2: (T, T)) -> T
where
    T: num_traits::Float,
{
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    (dx * dx + dy * dy).sqrt()
}

/// Calculate the center point of a rectangle
/// Used for rotation centers and other geometric calculations
/// 
/// # Arguments
/// * `x` - Left edge of rectangle
/// * `y` - Top edge of rectangle
/// * `width` - Width of rectangle
/// * `height` - Height of rectangle
/// 
/// # Returns
/// (center_x, center_y) tuple
pub fn calculate_center<T>(x: T, y: T, width: T, height: T) -> (T, T)
where
    T: num_traits::Float,
{
    (x + width / T::from(2.0).unwrap(), y + height / T::from(2.0).unwrap())
}

/// Create SVG transform attribute string for rotation
/// 
/// # Arguments
/// * `angle` - Rotation angle in degrees
/// * `center_x` - X coordinate of rotation center
/// * `center_y` - Y coordinate of rotation center
/// 
/// # Returns
/// SVG transform attribute string: "rotate(angle center_x center_y)"
pub fn create_svg_rotate_transform(angle: f64, center_x: f64, center_y: f64) -> String {
    format!("rotate({} {} {})", angle, center_x, center_y)
}

