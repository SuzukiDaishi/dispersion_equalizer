use crate::gui;
use crate::params::PluginParams;
use nih_plug::prelude::{Editor, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui};
use std::sync::Arc;

pub fn create(params: Arc<PluginParams>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    create_egui_editor(
        editor_state,
        (),
        |ctx, _| {
            gui::theme::apply(ctx);
        },
        move |ctx: &egui::Context, setter: &ParamSetter, _state| {
            gui::draw_editor(ctx, setter, &params);
        },
    )
}
