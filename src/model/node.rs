use crate::model::{RootNote, ScaleMode};
use nih_plug::prelude::Enum;
use serde::{Deserialize, Serialize};

pub const MAX_NODE_SLOTS: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Enum)]
pub enum NodeType {
    #[default]
    #[id = "bell"]
    #[name = "Bell Delay"]
    Bell,
    #[id = "lows"]
    #[name = "Low Shelf"]
    LowShelf,
    #[id = "highs"]
    #[name = "High Shelf"]
    HighShelf,
    #[id = "scale"]
    #[name = "Scale / Pentatonic"]
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeRuntimeParams {
    pub enabled: bool,
    pub node_type: NodeType,
    pub freq_hz: f32,
    pub amount_ms: f32,
    pub width_oct: f32,
    pub scale_root: RootNote,
    pub scale_mode: ScaleMode,
}

impl Default for NodeRuntimeParams {
    fn default() -> Self {
        Self {
            enabled: false,
            node_type: NodeType::Bell,
            freq_hz: 1000.0,
            amount_ms: 0.0,
            width_oct: 1.0,
            scale_root: RootNote::A,
            scale_mode: ScaleMode::MinorPentatonic,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeSnapshot {
    pub global_delay_ms: f32,
    pub wet: f32,
    pub output_gain_db: f32,
    pub max_sections: u32,
    pub nodes: [NodeRuntimeParams; MAX_NODE_SLOTS],
}

impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            global_delay_ms: 0.0,
            wet: 1.0,
            output_gain_db: 0.0,
            max_sections: 1024,
            nodes: [NodeRuntimeParams::default(); MAX_NODE_SLOTS],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeModel {
    pub slot: usize,
    pub id: u32,
    pub enabled: bool,
    pub node_type: NodeType,
    pub freq_hz: f32,
    pub amount_ms: f32,
    pub width_oct: f32,
    pub scale_root: RootNote,
    pub scale_mode: ScaleMode,
}

impl NodeModel {
    #[allow(dead_code)]
    pub fn from_runtime(slot: usize, node: NodeRuntimeParams) -> Self {
        Self {
            slot,
            id: slot as u32 + 1,
            enabled: node.enabled,
            node_type: node.node_type,
            freq_hz: node.freq_hz,
            amount_ms: node.amount_ms,
            width_oct: node.width_oct,
            scale_root: node.scale_root,
            scale_mode: node.scale_mode,
        }
    }

    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self.node_type {
            NodeType::Bell => "Bell",
            NodeType::LowShelf => "Low Shelf",
            NodeType::HighShelf => "High Shelf",
            NodeType::Scale => "Scale",
        }
    }
}
