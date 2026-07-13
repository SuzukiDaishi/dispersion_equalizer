use crate::gui;
use crate::params::{PluginParams, GUI_SCALE_RANGE};
use nih_plug::prelude::{Editor, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub fn create(params: Arc<PluginParams>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let build_params = params.clone();
    create_egui_editor(
        editor_state,
        (),
        move |ctx, _| {
            gui::theme::apply(ctx);

            // Restore the persisted UI scale. The window size stored in the editor state
            // already accounts for it.
            let scale = build_params
                .gui_scale
                .load(Ordering::Relaxed)
                .clamp(GUI_SCALE_RANGE.0, GUI_SCALE_RANGE.1);
            ctx.set_zoom_factor(scale);
        },
        move |ctx: &egui::Context, setter: &ParamSetter, _state| {
            gui::draw_editor(ctx, setter, &params);
        },
    )
}
