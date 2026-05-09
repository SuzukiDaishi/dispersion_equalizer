use crate::model::NodeModel;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetState {
    pub version: u32,
    pub selected_slot: Option<usize>,
    pub graph_max_ms: f32,
    pub nodes: Vec<NodeModel>,
}

impl Default for PresetState {
    fn default() -> Self {
        Self {
            version: 1,
            selected_slot: if cfg!(feature = "demo") { Some(0) } else { None },
            graph_max_ms: 1000.0,
            nodes: Vec::new(),
        }
    }
}
