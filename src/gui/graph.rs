use crate::compiler::descriptor::PreviewCurve;
use crate::compiler::{node_delay_at_freq, scale_frequencies, MAX_FREQ_HZ, MIN_FREQ_HZ};
use crate::gui::{selected_slot, update_preset_state};
use crate::model::{NodeRuntimeParams, NodeType, RuntimeSnapshot};
use crate::params::PluginParams;
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    self, epaint, pos2, vec2, Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    Ui,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum GraphAction {
    #[default]
    None,
    Select(Option<usize>),
    AddBell {
        freq_hz: f32,
        amount_ms: f32,
    },
    Move {
        slot: usize,
        freq_hz: f32,
        amount_ms: f32,
    },
    Width {
        slot: usize,
        width_oct: f32,
    },
    Remove(usize),
}

pub fn draw(
    ui: &mut Ui,
    ctx: &egui::Context,
    _setter: &ParamSetter,
    params: &PluginParams,
    snapshot: &RuntimeSnapshot,
    preview: &PreviewCurve,
    graph_max_ms: f32,
) -> GraphAction {
    let available = ui.available_size();
    let size = vec2(available.x.max(420.0), available.y.max(320.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let graph_rect = Rect::from_min_max(
        rect.left_top() + vec2(58.0, 20.0),
        rect.right_bottom() - vec2(22.0, 44.0),
    );

    painter.rect_filled(rect, 8.0, Color32::from_rgb(8, 12, 18));
    painter.rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, Color32::from_gray(42)),
        StrokeKind::Inside,
    );
    draw_grid(&painter, graph_rect, graph_max_ms);
    draw_global_line(&painter, graph_rect, snapshot.global_delay_ms, graph_max_ms);
    draw_scale_guides(&painter, graph_rect, snapshot, graph_max_ms);
    draw_node_influence(&painter, graph_rect, snapshot, graph_max_ms);
    draw_curve(
        &painter,
        graph_rect,
        &preview.actual_points,
        graph_max_ms,
        Stroke::new(2.0, Color32::from_rgb(240, 171, 252)),
    );
    draw_curve(
        &painter,
        graph_rect,
        &preview.target_points,
        graph_max_ms,
        Stroke::new(3.0, Color32::from_rgb(125, 211, 252)),
    );
    draw_nodes(
        &painter,
        graph_rect,
        snapshot,
        graph_max_ms,
        selected_slot(params),
    );

    let mut action = GraphAction::None;

    response.context_menu(|ui| {
        if let Some(slot) = selected_slot(params) {
            if ui.button("Remove").clicked() {
                action = GraphAction::Remove(slot);
                ui.close_menu();
            }
        } else {
            ui.label("No node selected");
        }
    });

    if response.hovered() {
        if ctx.input(|input| input.key_pressed(Key::Delete)) {
            if let Some(slot) = selected_slot(params) {
                action = GraphAction::Remove(slot);
            }
        }

        let scroll_y = ctx.input(|input| input.raw_scroll_delta.y);
        if scroll_y.abs() > 0.0 {
            if let Some(slot) = selected_slot(params) {
                let node = snapshot.nodes[slot];
                let factor = if scroll_y > 0.0 { 0.925 } else { 1.08 };
                let (min_w, max_w) = width_range(node.node_type);
                action = GraphAction::Width {
                    slot,
                    width_oct: (node.width_oct * factor).clamp(min_w, max_w),
                };
            }
        }
    }

    if response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if graph_rect.contains(pos) {
                action = GraphAction::AddBell {
                    freq_hz: x_to_freq(pos.x, graph_rect),
                    amount_ms: (y_to_ms(pos.y, graph_rect, graph_max_ms)
                        - snapshot.global_delay_ms)
                        .clamp(0.0, 1000.0),
                };
            }
        }
    } else if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            action = GraphAction::Select(hit_test(pos, graph_rect, snapshot, graph_max_ms));
            if let GraphAction::Select(slot) = action {
                update_preset_state(params, |state| state.selected_slot = slot);
            }
        }
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(slot) = selected_slot(params) {
                let node = snapshot.nodes[slot];
                let node_y =
                    ms_to_y(snapshot.global_delay_ms + node.amount_ms, graph_rect, graph_max_ms);
                let left_freq = node.freq_hz / 2.0_f32.powf(node.width_oct * 0.5);
                let right_freq = node.freq_hz * 2.0_f32.powf(node.width_oct * 0.5);
                let lx = freq_to_x(left_freq, graph_rect);
                let rx = freq_to_x(right_freq, graph_rect);

                let drag_id = egui::Id::new(("width_drag_side", slot));
                let drag_side: Option<bool> = if response.drag_started() {
                    let start = response.interact_pointer_pos().unwrap_or(pos);
                    let side = if (start.x - lx).abs() <= 10.0 && (start.y - node_y).abs() <= 10.0
                    {
                        Some(false) // left handle
                    } else if (start.x - rx).abs() <= 10.0 && (start.y - node_y).abs() <= 10.0 {
                        Some(true) // right handle
                    } else {
                        None
                    };
                    ctx.memory_mut(|m| m.data.insert_temp(drag_id, side));
                    side
                } else {
                    ctx.memory(|m| m.data.get_temp(drag_id)).flatten()
                };

                let (min_w, max_w) = width_range(node.node_type);
                match drag_side {
                    Some(false) => {
                        let new_left = x_to_freq(pos.x, graph_rect).clamp(MIN_FREQ_HZ, node.freq_hz * 0.999);
                        let new_width = ((node.freq_hz / new_left).log2() * 2.0).clamp(min_w, max_w);
                        action = GraphAction::Width { slot, width_oct: new_width };
                    }
                    Some(true) => {
                        let new_right = x_to_freq(pos.x, graph_rect).clamp(node.freq_hz * 1.001, MAX_FREQ_HZ);
                        let new_width = ((new_right / node.freq_hz).log2() * 2.0).clamp(min_w, max_w);
                        action = GraphAction::Width { slot, width_oct: new_width };
                    }
                    None => {
                        let freq_hz = x_to_freq(pos.x, graph_rect);
                        let amount_ms = (y_to_ms(pos.y, graph_rect, graph_max_ms)
                            - snapshot.global_delay_ms)
                            .clamp(0.0, 1000.0);
                        action = GraphAction::Move { slot, freq_hz, amount_ms };
                    }
                }
            }
        }
    }

    draw_status(ui, rect, preview);
    action
}

// ─── Drawing helpers ──────────────────────────────────────────────────────────

fn draw_grid(painter: &egui::Painter, rect: Rect, max_ms: f32) {
    let grid = Color32::from_rgba_unmultiplied(255, 255, 255, 26);
    let strong = Color32::from_rgba_unmultiplied(255, 255, 255, 54);
    let text = Color32::from_rgba_unmultiplied(238, 242, 255, 165);
    let ticks: [f32; 10] = [
        20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0, 20_000.0,
    ];

    for freq in ticks {
        let x = freq_to_x(freq, rect);
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(
                1.0,
                if (freq - 1000.0_f32).abs() < 1.0 { strong } else { grid },
            ),
        );
        painter.text(
            pos2(x, rect.bottom() + 18.0),
            egui::Align2::CENTER_CENTER,
            fmt_freq(freq),
            FontId::monospace(11.0),
            text,
        );
    }

    let step = if max_ms <= 100.0 {
        20.0
    } else if max_ms <= 250.0 {
        50.0
    } else if max_ms <= 500.0 {
        100.0
    } else {
        200.0
    };
    let mut ms = 0.0_f32;
    while ms <= max_ms + 0.1 {
        let y = ms_to_y(ms, rect, max_ms);
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, if ms == 0.0 { strong } else { grid }),
        );
        painter.text(
            pos2(rect.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{ms:.0} ms"),
            FontId::monospace(11.0),
            text,
        );
        ms += step;
    }

    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(72)),
        StrokeKind::Inside,
    );
    painter.text(
        pos2(rect.center().x, rect.bottom() + 34.0),
        egui::Align2::CENTER_CENTER,
        "Frequency",
        FontId::proportional(12.0),
        text,
    );
}

fn draw_global_line(painter: &egui::Painter, rect: Rect, global_ms: f32, max_ms: f32) {
    let y = ms_to_y(global_ms, rect, max_ms);
    painter.line_segment(
        [pos2(rect.left(), y), pos2(rect.right(), y)],
        Stroke::new(1.5, Color32::from_rgb(134, 239, 172)),
    );
}

/// Draw vertical guide lines at each scale tone frequency (for Scale nodes).
fn draw_scale_guides(
    painter: &egui::Painter,
    rect: Rect,
    snapshot: &RuntimeSnapshot,
    _max_ms: f32,
) {
    let color = Color32::from_rgba_unmultiplied(154, 247, 195, 64);
    for node in snapshot.nodes.iter() {
        if !node.enabled || node.node_type != NodeType::Scale {
            continue;
        }
        for f in scale_frequencies(node) {
            if f >= MIN_FREQ_HZ && f <= MAX_FREQ_HZ {
                let x = freq_to_x(f, rect);
                painter.line_segment(
                    [pos2(x, rect.top()), pos2(x, rect.bottom())],
                    Stroke::new(1.0, color),
                );
            }
        }
    }
}

fn draw_node_influence(
    painter: &egui::Painter,
    rect: Rect,
    snapshot: &RuntimeSnapshot,
    max_ms: f32,
) {
    const STEPS: usize = 96;
    let baseline_y = ms_to_y(snapshot.global_delay_ms, rect, max_ms);

    for node in snapshot.nodes.iter() {
        if !node.enabled {
            continue;
        }
        let fill = if node.node_type == NodeType::Scale {
            Color32::from_rgba_unmultiplied(154, 247, 195, 22)
        } else {
            Color32::from_rgba_unmultiplied(125, 211, 252, 24)
        };
        let mut mesh = epaint::Mesh::default();
        for i in 0..(STEPS - 1) {
            let (x0, y0) = node_curve_xy(i, node, snapshot, rect, max_ms);
            let (x1, y1) = node_curve_xy(i + 1, node, snapshot, rect, max_ms);
            let b = mesh.vertices.len() as u32;
            for &(px, py) in &[(x0, y0), (x1, y1), (x0, baseline_y), (x1, baseline_y)] {
                mesh.vertices.push(epaint::Vertex {
                    pos: pos2(px, py),
                    uv: epaint::WHITE_UV,
                    color: fill,
                });
            }
            mesh.indices
                .extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
        }
        painter.add(Shape::Mesh(std::sync::Arc::new(mesh)));
    }
}

fn node_curve_xy(
    index: usize,
    node: &NodeRuntimeParams,
    snapshot: &RuntimeSnapshot,
    rect: Rect,
    max_ms: f32,
) -> (f32, f32) {
    let t = index as f32 / 95.0;
    let freq = log_lerp(MIN_FREQ_HZ, MAX_FREQ_HZ, t);
    let ms = snapshot.global_delay_ms + node_delay_at_freq(node, freq);
    (freq_to_x(freq, rect), ms_to_y(ms, rect, max_ms))
}

fn draw_curve(
    painter: &egui::Painter,
    rect: Rect,
    points: &[[f32; 2]],
    max_ms: f32,
    stroke: Stroke,
) {
    if points.len() < 2 {
        return;
    }
    let path: Vec<Pos2> = points
        .iter()
        .map(|point| pos2(freq_to_x(point[0], rect), ms_to_y(point[1], rect, max_ms)))
        .collect();
    painter.add(Shape::line(path, stroke));
}

fn draw_nodes(
    painter: &egui::Painter,
    rect: Rect,
    snapshot: &RuntimeSnapshot,
    max_ms: f32,
    selected: Option<usize>,
) {
    for (slot, node) in snapshot.nodes.iter().enumerate() {
        if !node.enabled {
            continue;
        }

        let x = freq_to_x(node.freq_hz, rect);
        let y = ms_to_y(snapshot.global_delay_ms + node.amount_ms, rect, max_ms);
        let is_selected = selected == Some(slot);

        // Node color: scale = green, others = cyan; selected = amber
        let node_color = if is_selected {
            Color32::from_rgb(251, 191, 36)
        } else if node.node_type == NodeType::Scale {
            Color32::from_rgb(154, 247, 195)
        } else {
            Color32::from_rgb(125, 211, 252)
        };

        if is_selected {
            let left_freq = node.freq_hz / 2.0_f32.powf(node.width_oct * 0.5);
            let right_freq = node.freq_hz * 2.0_f32.powf(node.width_oct * 0.5);
            let lx = freq_to_x(left_freq, rect);
            let rx = freq_to_x(right_freq, rect);
            painter.line_segment(
                [pos2(lx, y), pos2(rx, y)],
                Stroke::new(2.0, Color32::from_rgb(251, 191, 36)),
            );
            let handle_color = Color32::from_rgb(251, 191, 36);
            for hx in [lx, rx] {
                painter.rect_filled(
                    Rect::from_center_size(pos2(hx, y), vec2(5.0, 12.0)),
                    2.0,
                    handle_color,
                );
            }
        }

        painter.circle_filled(pos2(x, y), if is_selected { 8.0 } else { 6.0 }, node_color);
        painter.circle_stroke(pos2(x, y), 8.0, Stroke::new(1.0, Color32::WHITE));
        painter.text(
            pos2(x, y - 14.0),
            egui::Align2::CENTER_BOTTOM,
            fmt_freq(node.freq_hz),
            FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(238, 242, 255, 220),
        );
    }
}

fn draw_status(ui: &mut Ui, rect: Rect, preview: &PreviewCurve) {
    let painter = ui.painter();
    let text = format!(
        "All-pass sections: {}   Pure delay: {:.1} ms   Fit RMS: {:.2} ms",
        preview.section_count, preview.pure_delay_ms, preview.fit_error_ms
    );
    painter.text(
        rect.left_top() + vec2(14.0, 12.0),
        egui::Align2::LEFT_TOP,
        text,
        FontId::monospace(11.0),
        Color32::from_rgba_unmultiplied(238, 242, 255, 210),
    );
}

fn hit_test(pos: Pos2, rect: Rect, snapshot: &RuntimeSnapshot, max_ms: f32) -> Option<usize> {
    let mut best = None;
    let mut best_dist = 24.0_f32;
    for (slot, node) in snapshot.nodes.iter().enumerate() {
        if !node.enabled {
            continue;
        }
        let x = freq_to_x(node.freq_hz, rect);
        let y = ms_to_y(snapshot.global_delay_ms + node.amount_ms, rect, max_ms);
        let dist = pos.distance(pos2(x, y));
        if dist < best_dist {
            best_dist = dist;
            best = Some(slot);
        }
    }
    best
}

/// Width range per node type: Scale uses narrower range (0.01–0.6 oct).
fn width_range(node_type: NodeType) -> (f32, f32) {
    if node_type == NodeType::Scale {
        (0.01, 0.6)
    } else {
        (0.03, 6.0)
    }
}

// ─── Coordinate transforms ────────────────────────────────────────────────────

pub fn freq_to_x(freq: f32, rect: Rect) -> f32 {
    let t = ((freq.clamp(MIN_FREQ_HZ, MAX_FREQ_HZ).log10() - MIN_FREQ_HZ.log10())
        / (MAX_FREQ_HZ.log10() - MIN_FREQ_HZ.log10()))
    .clamp(0.0, 1.0);
    rect.left() + t * rect.width()
}

pub fn x_to_freq(x: f32, rect: Rect) -> f32 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    log_lerp(MIN_FREQ_HZ, MAX_FREQ_HZ, t)
}

pub fn ms_to_y(ms: f32, rect: Rect, max_ms: f32) -> f32 {
    let t = (ms / max_ms.max(1.0)).clamp(0.0, 1.0);
    rect.bottom() - t * rect.height()
}

pub fn y_to_ms(y: f32, rect: Rect, max_ms: f32) -> f32 {
    let t = ((rect.bottom() - y) / rect.height()).clamp(0.0, 1.0);
    t * max_ms.max(1.0)
}

fn log_lerp(min: f32, max: f32, t: f32) -> f32 {
    10.0_f32.powf(min.log10() + (max.log10() - min.log10()) * t.clamp(0.0, 1.0))
}

fn fmt_freq(freq: f32) -> String {
    if freq >= 1000.0 {
        format!("{:.1}k", freq / 1000.0).replace(".0k", "k")
    } else {
        format!("{:.0}", freq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_mapping_round_trips() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 400.0));
        for freq in [20.0, 100.0, 1000.0, 10_000.0, 20_000.0] {
            let x = freq_to_x(freq, rect);
            let back = x_to_freq(x, rect);
            assert!((back / freq - 1.0).abs() < 0.001);
        }
    }
}
