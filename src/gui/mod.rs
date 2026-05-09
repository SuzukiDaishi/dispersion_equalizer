pub mod graph;
pub mod inspector;
pub mod node_view;
pub mod theme;

use crate::compiler::compile_preview;
use crate::gui::graph::GraphAction;
use crate::model::{NodeType, PresetState, RootNote, ScaleMode};
use crate::params::PluginParams;
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::{egui, widgets};

pub fn draw_editor(
    _ui: &mut egui::Ui,
    ctx: &egui::Context,
    setter: &ParamSetter,
    params: &PluginParams,
) {
    let sample_rate = 48_000.0;
    let snapshot = params.runtime_snapshot();
    let graph_max_ms = preset_state(params).graph_max_ms;
    let preview = compile_preview(&snapshot, sample_rate, graph_max_ms);

    egui::TopBottomPanel::top("top-bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Dispersion Equalizer");
            ui.separator();
            if ui.button("Flat").clicked() {
                apply_preset(setter, params, BuiltinPreset::Flat);
            }
            if ui.button("Big Global").clicked() {
                apply_preset(setter, params, BuiltinPreset::BigGlobal);
            }
            if ui.button("Disperser").clicked() {
                apply_preset(setter, params, BuiltinPreset::Disperser);
            }
            if ui.button("Vocal Air").clicked() {
                apply_preset(setter, params, BuiltinPreset::VocalAir);
            }
            if ui.button("A minor Penta").clicked() {
                apply_preset(setter, params, BuiltinPreset::AMinorPenta);
            }
            if ui.button("Bass Push").clicked() {
                apply_preset(setter, params, BuiltinPreset::BassPush);
            }
        });
    });

    egui::SidePanel::right("inspector")
        .resizable(false)
        .default_width(290.0)
        .show(ctx, |ui| {
            inspector::draw(ui, setter, params, &snapshot, &preview);
        });

    egui::TopBottomPanel::bottom("bottom-bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Global");
            ui.add(
                widgets::ParamSlider::for_param(&params.global_delay_ms, setter).with_width(130.0),
            );
            ui.label("Wet");
            ui.add(widgets::ParamSlider::for_param(&params.wet, setter).with_width(100.0));
            ui.label("Output");
            ui.add(
                widgets::ParamSlider::for_param(&params.output_gain_db, setter).with_width(100.0),
            );
            ui.label("Max SOS");
            ui.add(
                widgets::ParamSlider::for_param(&params.max_sections, setter).with_width(110.0),
            );
        });
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        let action = graph::draw(ui, ctx, setter, params, &snapshot, &preview, graph_max_ms);
        handle_graph_action(setter, params, action);
    });
}

// ── Presets (matches preview.html) ────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum BuiltinPreset {
    Flat,
    BigGlobal,
    Disperser,
    VocalAir,
    AMinorPenta,
    BassPush,
}

fn apply_preset(setter: &ParamSetter, params: &PluginParams, preset: BuiltinPreset) {
    for node in &params.nodes {
        set_param(setter, &node.enabled, false);
    }
    set_param(setter, &params.global_delay_ms, 0.0);
    set_param(setter, &params.wet, 1.0);
    set_param(setter, &params.output_gain_db, 1.0);

    match preset {
        BuiltinPreset::Flat => {}
        BuiltinPreset::BigGlobal => {
            set_param(setter, &params.global_delay_ms, 650.0);
        }
        // Two Bell nodes (900 Hz 260 ms + 2400 Hz 180 ms) — matches preview.html "Disperser" preset
        BuiltinPreset::Disperser => {
            configure_node(setter, params, 0, NodeType::Bell, 900.0, 260.0, 1.0);
            configure_node(setter, params, 1, NodeType::Bell, 2400.0, 180.0, 0.9);
        }
        // High Shelf + Bell — "Vocal Air"
        BuiltinPreset::VocalAir => {
            configure_node(setter, params, 0, NodeType::HighShelf, 5200.0, 340.0, 1.1);
            configure_node(setter, params, 1, NodeType::Bell, 1700.0, 220.0, 0.85);
        }
        // A minor pentatonic scale node
        BuiltinPreset::AMinorPenta => {
            set_param(setter, &params.global_delay_ms, 60.0);
            if let Some(node) = params.nodes.get(0) {
                set_param(setter, &node.enabled, true);
                set_param(setter, &node.node_type, NodeType::Scale);
                set_param(setter, &node.freq_hz, 440.0);
                set_param(setter, &node.amount_ms, 430.0);
                set_param(setter, &node.width_oct, 0.055);
                set_param(setter, &node.scale_root, RootNote::A);
                set_param(setter, &node.scale_mode, ScaleMode::MinorPentatonic);
            }
        }
        // Low Shelf + Bell — "Bass Push"
        BuiltinPreset::BassPush => {
            configure_node(setter, params, 0, NodeType::LowShelf, 180.0, 340.0, 1.2);
            configure_node(setter, params, 1, NodeType::Bell, 65.0, 280.0, 0.8);
        }
    }

    let selected_slot = params.nodes.iter().position(|n| n.enabled.value());
    update_preset_state(params, |state| state.selected_slot = selected_slot);
}

// ── Graph action handler ──────────────────────────────────────────────────────

fn handle_graph_action(setter: &ParamSetter, params: &PluginParams, action: GraphAction) {
    match action {
        GraphAction::None => {}
        GraphAction::Select(slot) => {
            update_preset_state(params, |state| state.selected_slot = slot);
        }
        GraphAction::AddBell { freq_hz, amount_ms } => {
            if let Some(slot) = first_free_slot(params) {
                configure_node(setter, params, slot, NodeType::Bell, freq_hz, amount_ms, 1.0);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
        GraphAction::Move { slot, freq_hz, amount_ms } => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.freq_hz, freq_hz);
                set_param(setter, &node.amount_ms, amount_ms);
            }
        }
        GraphAction::Width { slot, width_oct } => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.width_oct, width_oct);
            }
        }
        GraphAction::Remove(slot) => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.enabled, false);
            }
            update_preset_state(params, |state| {
                state.selected_slot = params.nodes.iter().position(|n| n.enabled.value());
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Configure a node slot with common parameters.
pub fn configure_node(
    setter: &ParamSetter,
    params: &PluginParams,
    slot: usize,
    node_type: NodeType,
    freq_hz: f32,
    amount_ms: f32,
    width_oct: f32,
) {
    if let Some(node) = params.nodes.get(slot) {
        set_param(setter, &node.enabled, true);
        set_param(setter, &node.node_type, node_type);
        set_param(setter, &node.freq_hz, freq_hz);
        set_param(setter, &node.amount_ms, amount_ms);
        set_param(setter, &node.width_oct, width_oct);
    }
}

pub fn set_param<P>(setter: &ParamSetter, param: &P, value: P::Plain)
where
    P: Param,
{
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

pub fn first_free_slot(params: &PluginParams) -> Option<usize> {
    params.nodes.iter().position(|node| !node.enabled.value())
}

pub fn selected_slot(params: &PluginParams) -> Option<usize> {
    preset_state(params).selected_slot.filter(|slot| {
        params
            .nodes
            .get(*slot)
            .is_some_and(|node| node.enabled.value())
    })
}

pub fn preset_state(params: &PluginParams) -> PresetState {
    params
        .preset_state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

pub fn update_preset_state(params: &PluginParams, update: impl FnOnce(&mut PresetState)) {
    if let Ok(mut state) = params.preset_state.lock() {
        update(&mut state);
    }
}
