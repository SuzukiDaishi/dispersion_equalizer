use crate::compiler::descriptor::PreviewCurve;
use crate::gui::{
    configure_node, first_free_slot, node_view, selected_slot, set_param, update_preset_state,
};
use crate::model::{NodeType, RuntimeSnapshot};
use crate::params::PluginParams;
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::{egui, widgets};

pub fn draw(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &PluginParams,
    snapshot: &RuntimeSnapshot,
    preview: &PreviewCurve,
) {
    ui.label(egui::RichText::new("Inspector").size(16.0).strong());
    ui.separator();
    ui.label(format!("Sections: {}", preview.section_count));
    ui.label(format!("Pure delay: {:.1} ms", preview.pure_delay_ms));
    ui.label(format!("Fit RMS: {:.2} ms", preview.fit_error_ms));
    ui.add_space(4.0);

    // ── Add node buttons ──────────────────────────────────────────────────────
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Bell").clicked() {
            if let Some(slot) = first_free_slot(params) {
                configure_node(setter, params, slot, NodeType::Bell, 1000.0, 250.0, 1.0);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
        if ui.button("+ Low Shelf").clicked() {
            if let Some(slot) = first_free_slot(params) {
                configure_node(setter, params, slot, NodeType::LowShelf, 250.0, 300.0, 1.2);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
        if ui.button("+ High Shelf").clicked() {
            if let Some(slot) = first_free_slot(params) {
                configure_node(setter, params, slot, NodeType::HighShelf, 5000.0, 250.0, 1.1);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
        if ui.button("+ Pentatonic").clicked() {
            if let Some(slot) = first_free_slot(params) {
                configure_node(setter, params, slot, NodeType::Scale, 440.0, 350.0, 0.055);
                update_preset_state(params, |state| state.selected_slot = Some(slot));
            }
        }
    });

    // ── Node list ─────────────────────────────────────────────────────────────
    ui.add_space(6.0);
    ui.label("Nodes");
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .show(ui, |ui| {
            for (slot, node) in snapshot.nodes.iter().enumerate() {
                if !node.enabled {
                    continue;
                }
                let selected = selected_slot(params) == Some(slot);
                let label = match node.node_type {
                    NodeType::Scale => format!(
                        "{} {:02}: {} {:?}, {:.0} ms",
                        node_view::node_type_label(node.node_type),
                        slot + 1,
                        format!("{:?}", node.scale_root),
                        node.scale_mode,
                        node.amount_ms,
                    ),
                    _ => format!(
                        "{} {:02}: {:.0} Hz, {:.0} ms",
                        node_view::node_type_label(node.node_type),
                        slot + 1,
                        node.freq_hz,
                        node.amount_ms,
                    ),
                };
                if ui.selectable_label(selected, label).clicked() {
                    update_preset_state(params, |state| state.selected_slot = Some(slot));
                }
            }
        });

    // ── Selected node ─────────────────────────────────────────────────────────
    ui.separator();
    let Some(slot) = selected_slot(params) else {
        ui.label("No node selected");
        return;
    };
    let node_params = &params.nodes[slot];
    let node = snapshot.nodes[slot];

    ui.label(egui::RichText::new(format!("Node {:02}", slot + 1)).size(14.0).strong());
    ui.add(widgets::ParamSlider::for_param(&node_params.enabled, setter));
    ui.add(widgets::ParamSlider::for_param(&node_params.node_type, setter));
    ui.add(widgets::ParamSlider::for_param(&node_params.freq_hz, setter));
    ui.add(widgets::ParamSlider::for_param(&node_params.amount_ms, setter));
    ui.add(widgets::ParamSlider::for_param(&node_params.width_oct, setter));

    // Scale-specific controls
    if node.node_type == NodeType::Scale {
        ui.separator();
        ui.label("Scale");
        ui.add(widgets::ParamSlider::for_param(&node_params.scale_root, setter));
        ui.add(widgets::ParamSlider::for_param(&node_params.scale_mode, setter));
    }

    ui.horizontal(|ui| {
        if ui.button("Remove").clicked() {
            set_param(setter, &node_params.enabled, false);
            update_preset_state(params, |state| {
                state.selected_slot = params.nodes.iter().position(|node| node.enabled.value());
            });
        }
    });

    // ── Graph max ─────────────────────────────────────────────────────────────
    ui.separator();
    ui.label("Graph Max");
    let mut graph_max = crate::gui::preset_state(params).graph_max_ms;
    if ui
        .add(egui::Slider::new(&mut graph_max, 50.0..=1000.0).suffix(" ms"))
        .changed()
    {
        update_preset_state(params, |state| state.graph_max_ms = graph_max);
    }
}
