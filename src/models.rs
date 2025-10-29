use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcalidrawElement {
    pub id: String,
    #[serde(rename = "type")]
    pub element_type: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub strokeColor: String,
    pub backgroundColor: String,
    pub fillStyle: String,
    pub strokeWidth: f64,
    pub strokeStyle: String,
    pub roughness: f64,
    pub opacity: f64,
    pub groupIds: Vec<String>,
    pub frameId: Option<String>,
    pub index: String,
    pub roundness: Option<RoundnessType>,
    pub seed: i32,
    pub versionNonce: Option<i32>,
    pub isDeleted: bool,
    pub boundElements: Option<Vec<BoundElement>>,
    pub updated: i64,
    pub link: Option<String>,
    pub locked: bool,
    pub text: Option<String>,
    pub fontSize: Option<f64>,
    pub fontFamily: Option<i32>,
    pub textAlign: Option<String>,
    pub verticalAlign: Option<String>,
    pub containerId: Option<String>,
    pub originalText: Option<String>,
    pub lineHeight: Option<f64>,
    pub baseline: Option<f64>,
    pub startBinding: Option<Binding>,
    pub endBinding: Option<Binding>,
    pub startArrowType: Option<String>,
    pub endArrowType: Option<String>,
    pub startArrowhead: Option<String>,
    pub endArrowhead: Option<String>,
    pub points: Option<Vec<(f64, f64)>>,
    pub lastCommittedPoint: Option<Vec<f64>>,
    pub elbowed: Option<bool>,
    #[serde(default)]
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundnessType {
    #[serde(rename = "type")]
    pub roundness_type: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundElement {
    pub id: String,
    #[serde(rename = "type")]
    pub element_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub elementId: String,
    pub focus: f64,
    pub gap: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExcalidrawData {
    #[serde(rename = "type")]
    pub data_type: String,
    pub version: i32,
    pub versionNonce: Option<i32>,
    pub source: String,
    pub elements: Vec<ExcalidrawElement>,
    #[serde(default)]
    pub appState: HashMap<String, serde_json::Value>,
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
