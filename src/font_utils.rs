/// Get font family name based on Excalidraw font ID
/// Maps font IDs to family names that match the loaded fonts
/// 
/// Font ID mapping:
/// - None/0: "Excalifont" (default)
/// - 1: "Liberation Sans"
/// - 2: "Cascadia Code"
pub fn get_font_family(font_id: Option<i32>) -> &'static str {
    match font_id {
        Some(1) => "Liberation Sans",
        Some(2) => "Cascadia Code",
        _ => "Excalifont", // Default or ID 0
    }
}

