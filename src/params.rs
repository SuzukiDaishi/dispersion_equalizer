use crate::model::{
    NodeRuntimeParams, NodeType, PresetState, RootNote, RuntimeSnapshot, ScaleMode, MAX_NODE_SLOTS,
};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::{Arc, Mutex};

#[derive(Params)]
pub struct PluginParams {
    #[persist = "editor"]
    pub editor_state: Arc<EguiState>,

    #[persist = "preset"]
    pub preset_state: Arc<Mutex<PresetState>>,

    #[id = "gdel"]
    pub global_delay_ms: FloatParam,

    #[id = "wet"]
    pub wet: FloatParam,

    #[id = "out"]
    pub output_gain_db: FloatParam,

    #[id = "msos"]
    pub max_sections: IntParam,

    #[id = "xfms"]
    pub transition_ms: FloatParam,

    #[nested(array, group = "Nodes")]
    pub nodes: [NodeParams; MAX_NODE_SLOTS],
}

#[derive(Params)]
pub struct NodeParams {
    #[id = "en"]
    pub enabled: BoolParam,

    #[id = "type"]
    pub node_type: EnumParam<NodeType>,

    #[id = "freq"]
    pub freq_hz: FloatParam,

    #[id = "amt"]
    pub amount_ms: FloatParam,

    #[id = "width"]
    pub width_oct: FloatParam,

    #[id = "root"]
    pub scale_root: EnumParam<RootNote>,

    #[id = "scale"]
    pub scale_mode: EnumParam<ScaleMode>,
}

impl Default for PluginParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1024, 680),
            preset_state: Arc::new(Mutex::new(PresetState::default())),
            global_delay_ms: FloatParam::new(
                "Global Delay",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1000.0,
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            wet: FloatParam::new("Wet", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            output_gain_db: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-24.0),
                    max: util::db_to_gain(24.0),
                    factor: FloatRange::gain_skew_factor(-24.0, 24.0),
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Logarithmic(10.0))
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            max_sections: IntParam::new(
                "Max SOS",
                1024,
                IntRange::Linear {
                    min: 8,
                    max: crate::dsp::MAX_RUNTIME_SECTIONS as i32,
                },
            ),
            transition_ms: FloatParam::new(
                "Transition",
                50.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0)
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            nodes: std::array::from_fn(NodeParams::new),
        }
    }
}

impl PluginParams {
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            global_delay_ms: self.global_delay_ms.value().clamp(0.0, 1000.0),
            wet: self.wet.value().clamp(0.0, 1.0),
            output_gain_db: util::gain_to_db(self.output_gain_db.value()),
            max_sections: self
                .max_sections
                .value()
                .clamp(8, crate::dsp::MAX_RUNTIME_SECTIONS as i32) as u32,
            nodes: std::array::from_fn(|index| self.nodes[index].runtime_params()),
        }
    }

    /// スムース前の目標値で snapshot を作る。rebuild 判定と greedy 計算に使う。
    /// smoother のアニメーション中でも値が変化しないため毎フレーム rebuild が
    /// 走るのを防ぐ。
    pub fn target_snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            global_delay_ms: self
                .global_delay_ms
                .unmodulated_plain_value()
                .clamp(0.0, 1000.0),
            wet: self.wet.unmodulated_plain_value().clamp(0.0, 1.0),
            output_gain_db: util::gain_to_db(self.output_gain_db.unmodulated_plain_value()),
            max_sections: self
                .max_sections
                .unmodulated_plain_value()
                .clamp(8, crate::dsp::MAX_RUNTIME_SECTIONS as i32) as u32,
            nodes: std::array::from_fn(|index| self.nodes[index].target_params()),
        }
    }
}

impl NodeParams {
    pub fn new(index: usize) -> Self {
        let one_based = index + 1;
        let demo_node0 = cfg!(feature = "demo") && index == 0;
        Self {
            enabled: BoolParam::new(format!("Node {one_based} Enabled"), demo_node0),
            node_type: EnumParam::new(format!("Node {one_based} Type"), NodeType::Bell),
            freq_hz: FloatParam::new(
                format!("Node {one_based} Frequency"),
                if demo_node0 { 900.0 } else { 1000.0 },
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(2))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            amount_ms: FloatParam::new(
                format!("Node {one_based} Amount"),
                if demo_node0 { 800.0 } else { 250.0 },
                FloatRange::Linear {
                    min: 0.0,
                    max: 1000.0,
                },
            )
            .with_unit(" ms")
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(15.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            width_oct: FloatParam::new(
                format!("Node {one_based} Width"),
                if demo_node0 { 1.0 } else { 1.0 },
                FloatRange::Linear {
                    min: 0.01,
                    max: 6.0,
                },
            )
            .with_unit(" oct")
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            scale_root: EnumParam::new(format!("Node {one_based} Root"), RootNote::A),
            scale_mode: EnumParam::new(
                format!("Node {one_based} Scale"),
                ScaleMode::MinorPentatonic,
            ),
        }
    }

    pub fn runtime_params(&self) -> NodeRuntimeParams {
        NodeRuntimeParams {
            enabled: self.enabled.value(),
            node_type: self.node_type.value(),
            freq_hz: self.freq_hz.value().clamp(20.0, 20000.0),
            amount_ms: self.amount_ms.value().clamp(0.0, 1000.0),
            width_oct: self.width_oct.value().clamp(0.01, 6.0),
            scale_root: self.scale_root.value(),
            scale_mode: self.scale_mode.value(),
        }
    }

    pub fn target_params(&self) -> NodeRuntimeParams {
        NodeRuntimeParams {
            enabled: self.enabled.value(),
            node_type: self.node_type.value(),
            freq_hz: self.freq_hz.unmodulated_plain_value().clamp(20.0, 20000.0),
            amount_ms: self.amount_ms.unmodulated_plain_value().clamp(0.0, 1000.0),
            width_oct: self.width_oct.unmodulated_plain_value().clamp(0.01, 6.0),
            scale_root: self.scale_root.value(),
            scale_mode: self.scale_mode.value(),
        }
    }
}
