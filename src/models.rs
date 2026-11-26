use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Style/geometry defaults matching Excalidraw so partial or older documents
// (which omit fields such as `index`) still deserialize.
fn default_opacity() -> f64 {
    100.0
}
fn default_stroke_width() -> f64 {
    1.0
}
fn default_roughness() -> f64 {
    1.0
}
fn default_stroke_color() -> String {
    "#1e1e1e".to_string()
}
fn default_transparent() -> String {
    "transparent".to_string()
}
fn default_fill_style() -> String {
    "hachure".to_string()
}
fn default_stroke_style() -> String {
    "solid".to_string()
}
fn deserialize_pressures<'de, D>(deserializer: D) -> Result<Option<Vec<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<f64>>::deserialize(deserializer).unwrap_or_default())
}

fn deserialize_created<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?.and_then(|value| value.as_f64()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExcalidrawElement {
    pub id: String,
    #[serde(rename = "type")]
    pub element_type: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: String,
    pub stroke_width: f64,
    pub stroke_style: String,
    pub roughness: f64,
    pub opacity: f64,
    pub group_ids: Vec<String>,
    pub frame_id: Option<String>,
    pub index: String,
    pub roundness: Option<RoundnessType>,
    pub seed: i32,
    pub version_nonce: Option<i32>,
    pub is_deleted: bool,
    pub bound_elements: Option<Vec<BoundElement>>,
    pub updated: i64,
    pub link: Option<String>,
    pub locked: bool,
    pub name: Option<String>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<i32>,
    pub text_align: Option<String>,
    pub vertical_align: Option<String>,
    pub container_id: Option<String>,
    pub label_position: Option<f64>,
    pub original_text: Option<String>,
    pub line_height: Option<f64>,
    pub baseline: Option<f64>,
    pub start_binding: Option<Binding>,
    pub end_binding: Option<Binding>,
    pub start_arrow_type: Option<String>,
    pub end_arrow_type: Option<String>,
    pub start_arrowhead: Option<String>,
    pub end_arrowhead: Option<String>,
    pub points: Option<Vec<(f64, f64)>>,
    pub last_committed_point: Option<Vec<f64>>,
    pub elbowed: Option<bool>,
    // Freehand element fields (type="freedraw")
    #[serde(default, deserialize_with = "deserialize_pressures")]
    pub pressures: Option<Vec<f64>>,
    pub simulate_pressure: bool,
    pub stroke_options: Option<StrokeOptions>,
    // Sticky-note creation timestamp in milliseconds since the Unix epoch.
    #[serde(default, deserialize_with = "deserialize_created")]
    pub created: Option<f64>,
    // Image element fields (type="image")
    pub file_id: Option<String>,
    pub scale: Option<Vec<f64>>,
    pub crop: Option<serde_json::Value>,
    pub version: Option<i32>,
}

impl Default for ExcalidrawElement {
    fn default() -> Self {
        ExcalidrawElement {
            id: String::new(),
            element_type: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            angle: 0.0,
            stroke_color: default_stroke_color(),
            background_color: default_transparent(),
            fill_style: default_fill_style(),
            stroke_width: default_stroke_width(),
            stroke_style: default_stroke_style(),
            roughness: default_roughness(),
            opacity: default_opacity(),
            group_ids: Vec::new(),
            frame_id: None,
            index: String::new(),
            roundness: None,
            seed: 0,
            version_nonce: None,
            is_deleted: false,
            bound_elements: None,
            updated: 0,
            link: None,
            locked: false,
            name: None,
            text: None,
            font_size: None,
            font_family: None,
            text_align: None,
            vertical_align: None,
            container_id: None,
            label_position: None,
            original_text: None,
            line_height: None,
            baseline: None,
            start_binding: None,
            end_binding: None,
            start_arrow_type: None,
            end_arrow_type: None,
            start_arrowhead: None,
            end_arrowhead: None,
            points: None,
            last_committed_point: None,
            elbowed: None,
            pressures: None,
            simulate_pressure: false,
            stroke_options: None,
            created: None,
            file_id: None,
            scale: None,
            crop: None,
            version: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StrokeOptions {
    pub variability: Option<String>,
    pub streamline: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundnessType {
    #[serde(rename = "type")]
    pub roundness_type: i32,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundElement {
    pub id: String,
    #[serde(rename = "type")]
    pub element_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub element_id: String,
    #[serde(default)]
    pub focus: Option<f64>,
    #[serde(default)]
    pub gap: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcalidrawData {
    #[serde(rename = "type")]
    pub data_type: String,
    pub version: Option<i32>,
    pub version_nonce: Option<i32>,
    pub source: Option<String>,
    pub elements: Vec<ExcalidrawElement>,
    #[serde(default)]
    pub app_state: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub files: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewBox {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}
