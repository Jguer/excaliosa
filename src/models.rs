use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<i32>,
    pub text_align: Option<String>,
    pub vertical_align: Option<String>,
    pub container_id: Option<String>,
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
    #[serde(default)]
    pub version: Option<i32>,
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
    pub focus: f64,
    pub gap: f64,
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
