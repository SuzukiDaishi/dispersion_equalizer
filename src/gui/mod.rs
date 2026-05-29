pub mod graph;
pub mod inspector;
pub mod node_view;
pub mod theme;

use crate::compiler::compile_preview;
use crate::gui::graph::GraphAction;
use crate::model::{
    NodeModel, NodeRuntimeParams, NodeType, PresetState, RootNote, RuntimeSnapshot, ScaleMode,
    MAX_NODE_SLOTS,
};
use crate::params::PluginParams;
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::{egui, widgets};

const UI_VERSION_TEXT: &str = env!("CARGO_PKG_VERSION");

pub fn draw_editor(ctx: &egui::Context, setter: &ParamSetter, params: &PluginParams) {
    let sample_rate = 48_000.0;
    let host_sync_repaint_key = egui::Id::new("host_sync_repaint_frames");
    let mut graph_max_ms = preset_state(params).graph_max_ms;
    let mut snapshot = ui_snapshot(params);
    let mut preview = compile_preview(&snapshot, sample_rate, graph_max_ms);

    // Helper: refresh snapshot/preview after any state mutation so downstream
    // panels see the updated state in the same frame.
    macro_rules! refresh {
        () => {{
            graph_max_ms = preset_state(params).graph_max_ms;
            snapshot = ui_snapshot(params);
            preview = compile_preview(&snapshot, sample_rate, graph_max_ms);
            ctx.request_repaint();
        }};
    }

    let preset_changed = {
        let mut changed = false;
        egui::TopBottomPanel::top("top-bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Dispersion Equalizer");
                ui.label(egui::RichText::new(format!("v{}", UI_VERSION_TEXT)).weak());
                ui.separator();
                if ui.button("Flat").clicked() {
                    apply_preset(setter, params, BuiltinPreset::Flat);
                    changed = true;
                }
                if ui.button("Big Global").clicked() {
                    apply_preset(setter, params, BuiltinPreset::BigGlobal);
                    changed = true;
                }
                if ui.button("Spread").clicked() {
                    apply_preset(setter, params, BuiltinPreset::PhaseSpread);
                    changed = true;
                }
                if ui.button("Vocal Air").clicked() {
                    apply_preset(setter, params, BuiltinPreset::VocalAir);
                    changed = true;
                }
                if ui.button("A minor Penta").clicked() {
                    apply_preset(setter, params, BuiltinPreset::AMinorPenta);
                    changed = true;
                }
                if ui.button("Bass Push").clicked() {
                    apply_preset(setter, params, BuiltinPreset::BassPush);
                    changed = true;
                }
            });
        });
        changed
    };

    // Refresh before Inspector so it sees the preset change in the same frame.
    if preset_changed {
        refresh!();
    }

    let inspector_changed = egui::SidePanel::right("inspector")
        .resizable(false)
        .default_width(290.0)
        .show(ctx, |ui| {
            inspector::draw(ui, setter, params, &snapshot, &preview)
        })
        .inner;

    // Refresh before Graph so it sees the inspector change in the same frame.
    if inspector_changed {
        refresh!();
    }

    let bottom_changed = egui::TopBottomPanel::bottom("bottom-bar").show(ctx, |ui| {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Global");
            changed |= ui
                .add(widgets::ParamSlider::for_param(&params.global_delay_ms, setter).with_width(130.0))
                .changed();
            ui.label("Wet");
            changed |= ui.add(widgets::ParamSlider::for_param(&params.wet, setter).with_width(100.0)).changed();
            ui.label("Gain");
            changed |= ui
                .add(widgets::ParamSlider::for_param(&params.output_gain_db, setter).with_width(100.0))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Max SOS");
            changed |= ui
                .add(widgets::ParamSlider::for_param(&params.max_sections, setter).with_width(110.0))
                .changed();
            ui.label("Transition");
            changed |= ui
                .add(widgets::ParamSlider::for_param(&params.transition_ms, setter).with_width(100.0))
                .changed();
            ui.separator();
            let mut duck = params.peak_guard.unmodulated_plain_value();
            if ui
                .checkbox(&mut duck, "Peak Guard")
                .on_hover_text("Brickwall limiter on the wet channel. Prevents clipping during delay changes.")
                .changed()
            {
                set_param(setter, &params.peak_guard, duck);
                changed = true;
            }
        });
        changed
    }).inner;

    if bottom_changed {
        refresh!();
    }

    let mut graph_changed = false;
    egui::CentralPanel::default().show(ctx, |ui| {
        let action = graph::draw(ui, ctx, setter, params, &snapshot, &preview, graph_max_ms);
        handle_graph_action(setter, params, action);
        if !matches!(action, GraphAction::None) {
            graph_changed = true;
            refresh!();
        }
    });

    // Host roundtrip for AUv2 can land a few frames later. Keep repainting for
    // a short window after interactions so stale snapshots get a chance to catch up.
    if preset_changed || inspector_changed || bottom_changed || graph_changed {
        ctx.memory_mut(|memory| memory.data.insert_temp(host_sync_repaint_key, 10_u8));
    }
    let pending = ctx
        .memory(|memory| memory.data.get_temp::<u8>(host_sync_repaint_key))
        .unwrap_or(0);
    if pending > 0 {
        ctx.memory_mut(|memory| memory.data.insert_temp(host_sync_repaint_key, pending - 1));
        ctx.request_repaint();
    }
}

// ── Presets (matches preview.html) ────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum BuiltinPreset {
    Flat,
    BigGlobal,
    PhaseSpread,
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
        // Two Bell nodes for a broad phase-spread preset.
        BuiltinPreset::PhaseSpread => {
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
                set_param(setter, &node.node_type, NodeType::Scale);
                set_param(setter, &node.freq_hz, 440.0);
                set_param(setter, &node.amount_ms, 430.0);
                set_param(setter, &node.width_oct, 0.055);
                set_param(setter, &node.scale_root, RootNote::A);
                set_param(setter, &node.scale_mode, ScaleMode::MinorPentatonic);
                set_param(setter, &node.enabled, true);
            }
        }
        // Low Shelf + Bell — "Bass Push"
        BuiltinPreset::BassPush => {
            configure_node(setter, params, 0, NodeType::LowShelf, 180.0, 340.0, 1.2);
            configure_node(setter, params, 1, NodeType::Bell, 65.0, 280.0, 0.8);
        }
    }

    sync_state_nodes_from_target(params);

    let selected_slot = params
        .nodes
        .iter()
        .position(|n| n.enabled.unmodulated_plain_value());
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
                configure_node(
                    setter,
                    params,
                    slot,
                    NodeType::Bell,
                    freq_hz,
                    amount_ms,
                    1.0,
                );
                upsert_state_node(params, slot, NodeType::Bell, freq_hz, amount_ms, 1.0, None, None, true);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
        GraphAction::Move {
            slot,
            freq_hz,
            amount_ms,
        } => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.freq_hz, freq_hz);
                set_param(setter, &node.amount_ms, amount_ms);
                if let Some(existing) = state_node_at(params, slot) {
                    upsert_state_node(
                        params,
                        slot,
                        existing.node_type,
                        freq_hz,
                        amount_ms,
                        existing.width_oct,
                        Some(existing.scale_root),
                        Some(existing.scale_mode),
                        existing.enabled,
                    );
                }
            }
        }
        GraphAction::Width { slot, width_oct } => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.width_oct, width_oct);
                if let Some(existing) = state_node_at(params, slot) {
                    upsert_state_node(
                        params,
                        slot,
                        existing.node_type,
                        existing.freq_hz,
                        existing.amount_ms,
                        width_oct,
                        Some(existing.scale_root),
                        Some(existing.scale_mode),
                        existing.enabled,
                    );
                }
            }
        }
        GraphAction::Remove(slot) => {
            if let Some(node) = params.nodes.get(slot) {
                set_param(setter, &node.enabled, false);
            }
            set_state_node_enabled(params, slot, false);
            update_preset_state(params, |state| {
                state.selected_slot = params
                    .nodes
                    .iter()
                    .position(|n| n.enabled.unmodulated_plain_value());
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
        // Set dependent parameters first, then enable the node as the final step.
        // Some hosts (notably Steinberg hosts) can be picky about batched UI edits
        // when a gate/enable parameter flips before related values are written.
        set_param(setter, &node.node_type, node_type);
        set_param(setter, &node.freq_hz, freq_hz);
        set_param(setter, &node.amount_ms, amount_ms);
        set_param(setter, &node.width_oct, width_oct);
        set_param(setter, &node.enabled, true);

        upsert_state_node(
            params,
            slot,
            node_type,
            freq_hz,
            amount_ms,
            width_oct,
            Some(node.scale_root.unmodulated_plain_value()),
            Some(node.scale_mode.unmodulated_plain_value()),
            true,
        );
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
    let state = preset_state(params);
    if !state.nodes.is_empty() {
        for slot in 0..MAX_NODE_SLOTS {
            let occupied = state.nodes.iter().any(|node| node.slot == slot && node.enabled);
            if !occupied {
                return Some(slot);
            }
        }
        return None;
    }

    params
        .nodes
        .iter()
        .position(|node| !node.enabled.unmodulated_plain_value())
}

pub fn selected_slot(params: &PluginParams) -> Option<usize> {
    let state = preset_state(params);
    preset_state(params).selected_slot.filter(|slot| {
        if let Some(state_node) = state.nodes.iter().find(|node| node.slot == *slot) {
            return state_node.enabled;
        }

        params.nodes.get(*slot).is_some_and(|node| node.enabled.unmodulated_plain_value())
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

pub fn set_state_node_enabled(params: &PluginParams, slot: usize, enabled: bool) {
    update_preset_state(params, |state| {
        if let Some(existing) = state.nodes.iter_mut().find(|node| node.slot == slot) {
            existing.enabled = enabled;
        }
    });
}

fn state_node_at(params: &PluginParams, slot: usize) -> Option<NodeRuntimeParams> {
    let state = preset_state(params);
    state.nodes.iter().find(|node| node.slot == slot).map(|node| NodeRuntimeParams {
        enabled: node.enabled,
        node_type: node.node_type,
        freq_hz: node.freq_hz,
        amount_ms: node.amount_ms,
        width_oct: node.width_oct,
        scale_root: node.scale_root,
        scale_mode: node.scale_mode,
    })
}

fn upsert_state_node(
    params: &PluginParams,
    slot: usize,
    node_type: NodeType,
    freq_hz: f32,
    amount_ms: f32,
    width_oct: f32,
    scale_root: Option<RootNote>,
    scale_mode: Option<ScaleMode>,
    enabled: bool,
) {
    update_preset_state(params, |state| {
        let model = NodeModel {
            slot,
            id: slot as u32 + 1,
            enabled,
            node_type,
            freq_hz,
            amount_ms,
            width_oct,
            scale_root: scale_root.unwrap_or(RootNote::A),
            scale_mode: scale_mode.unwrap_or(ScaleMode::MinorPentatonic),
        };

        if let Some(existing) = state.nodes.iter_mut().find(|node| node.slot == slot) {
            *existing = model;
        } else {
            state.nodes.push(model);
        }
    });
}

fn sync_state_nodes_from_target(params: &PluginParams) {
    let snapshot = params.target_snapshot();
    update_preset_state(params, |state| {
        state.nodes = snapshot
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(slot, node)| {
                if !node.enabled {
                    return None;
                }
                Some(NodeModel {
                    slot,
                    id: slot as u32 + 1,
                    enabled: node.enabled,
                    node_type: node.node_type,
                    freq_hz: node.freq_hz,
                    amount_ms: node.amount_ms,
                    width_oct: node.width_oct,
                    scale_root: node.scale_root,
                    scale_mode: node.scale_mode,
                })
            })
            .collect();
    });
}

fn ui_snapshot(params: &PluginParams) -> RuntimeSnapshot {
    let mut snapshot = params.target_snapshot();
    let state = preset_state(params);

    for node in &state.nodes {
        if let Some(slot) = snapshot.nodes.get_mut(node.slot) {
            *slot = NodeRuntimeParams {
                enabled: node.enabled,
                node_type: node.node_type,
                freq_hz: node.freq_hz,
                amount_ms: node.amount_ms,
                width_oct: node.width_oct,
                scale_root: node.scale_root,
                scale_mode: node.scale_mode,
            };
        }
    }

    snapshot
}
