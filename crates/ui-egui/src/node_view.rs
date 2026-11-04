//! A feature dependency view. Arrangement is presentation state; edits use normal commands.
use std::collections::{BTreeMap, BTreeSet};

use egui::{Align2, FontId, Id, Pos2, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;
use solidcraft_engine::schema::view_navigation::NodeGraph;

use crate::{SolveApp, i18n, theme::Tokens};

#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    uid: u64,
    revision: Option<u64>,
    graph: NodeGraph,
    positions: BTreeMap<u64, Pos2>,
    zoom: Option<f32>,
    error: Option<String>,
}

impl State {
    fn refresh(&mut self, app: &mut SolveApp) {
        if self.uid != app.session.uid {
            self.uid = app.session.uid;
            self.revision = None;
            self.positions.clear();
        }
        if self.revision == Some(app.session.revision) {
            return;
        }
        match app
            .session
            .execute("view.node_graph", &json!({}))
            .map_err(|e| e.to_string())
            .and_then(|value| serde_json::from_value(value).map_err(|e| e.to_string()))
        {
            Ok(graph) => {
                self.graph = graph;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.revision = Some(app.session.revision);
        self.arrange(false);
    }

    fn arrange(&mut self, reset: bool) {
        if reset {
            self.positions.clear();
        }
        let live: BTreeSet<_> = self.graph.nodes.iter().map(|node| node.id).collect();
        self.positions.retain(|id, _| live.contains(id));
        let mut columns: BTreeMap<u64, usize> = BTreeMap::new();
        let mut rows = BTreeMap::new();
        for node in &self.graph.nodes {
            let column = self
                .graph
                .links
                .iter()
                .filter(|link| link.target == node.id)
                .filter_map(|link| columns.get(&link.source).map(|column| column + 1))
                .max()
                .unwrap_or(0usize);
            columns.insert(node.id, column);
            let row = rows.entry(column).or_insert(0usize);
            self.positions.entry(node.id).or_insert_with(|| pos2(32.0 + column as f32 * 264.0, 32.0 + *row as f32 * 124.0));
            *row += 1;
        }
    }
}

enum Action {
    Pick(u64),
    Edit(u64),
    Suppress(u64),
}

pub(crate) fn show(app: &mut SolveApp, ctx: &egui::Context) {
    if !app.node_view.open {
        return;
    }
    let mut state = std::mem::take(&mut app.node_view);
    state.refresh(app);
    let t = Tokens::get();
    let selected: BTreeSet<_> = app
        .session
        .selection
        .iter()
        .filter_map(|item| match item {
            solidcraft_engine::Sel::Feature { id } => Some(*id),
            solidcraft_engine::Sel::Body { name } => app.session.model.state().body(name).map(|body| body.feature),
            _ => None,
        })
        .collect();
    let mut action = None;
    let closed = crate::ui_kit::Window::new(Id::new(("feature_node_view", state.uid)), "graph", i18n::tr("node_graph.title")).cards(4.0).show_body(
        ctx,
        |ui| {
            ui.horizontal_wrapped(|ui| {
                let layout = crate::ui_kit::button(ui, i18n::tr("node_graph.arrange"), crate::ui_kit::Button::Normal);
                crate::scenario::publish_handle("node_graph:arrange", layout.rect.center());
                if layout.clicked() {
                    state.arrange(true);
                }
                if crate::ui_kit::button(ui, "−", crate::ui_kit::Button::Normal).on_hover_text(i18n::tr("node_graph.zoom_out")).clicked() {
                    state.zoom = Some((state.zoom.unwrap_or(1.0) - 0.1).max(0.5));
                }
                ui.label(format!("{:.0}%", state.zoom.unwrap_or(1.0) * 100.0));
                if crate::ui_kit::button(ui, "+", crate::ui_kit::Button::Normal).on_hover_text(i18n::tr("node_graph.zoom_in")).clicked() {
                    state.zoom = Some((state.zoom.unwrap_or(1.0) + 0.1).min(1.6));
                }
                let picked = selected.first().copied();
                let edit = crate::ui_kit::button(ui, i18n::tr("node_graph.edit"), crate::ui_kit::Button::Enabled(picked.is_some()));
                crate::scenario::publish_handle("node_graph:edit", edit.rect.center());
                if edit.clicked()
                    && let Some(id) = picked
                {
                    action = Some(Action::Edit(id));
                }
                if crate::ui_kit::button(ui, i18n::tr("node_graph.toggle_suppression"), crate::ui_kit::Button::Enabled(picked.is_some())).clicked()
                    && let Some(id) = picked
                {
                    action = Some(Action::Suppress(id));
                }
                ui.separator();
                ui.label(i18n::format("node_graph.count", &[("count", &state.graph.nodes.len().to_string())]));
            });
            ui.label(egui::RichText::new(i18n::tr("node_graph.hint")).color(t.form_dim).size(t.font_small));
            if let Some(error) = &state.error {
                ui.colored_label(t.form_error, error);
            }
            if state.graph.nodes.is_empty() {
                ui.label(i18n::tr("node_graph.empty"));
                return;
            }
            let zoom = state.zoom.unwrap_or(1.0);
            let size = vec2(200.0, 88.0);
            let content_size =
                state.positions.values().fold(vec2(0.0, 0.0), |extent, point| extent.max(point.to_vec2() + size + vec2(48.0, 48.0))) * zoom;
            // A finite, scrollable document rather than a separate camera. egui owns
            // scrolling; dragging a card only changes its presentation position.
            egui::ScrollArea::both()
                .id_salt(("node_graph_scroll", state.uid))
                .max_height((ctx.content_rect().height() * 0.48).clamp(160.0, 440.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (canvas, _) = ui.allocate_exact_size(content_size.max(vec2(ui.available_width(), 260.0)), Sense::hover());
                    crate::scenario::publish_rect("node_graph:canvas", canvas);
                    let painter = ui.painter_at(canvas);
                    painter.rect_filled(canvas, t.control_radius, t.form_fill);
                    let rects: BTreeMap<_, _> = state
                        .positions
                        .iter()
                        .map(|(id, point)| (*id, Rect::from_min_size(canvas.min + point.to_vec2() * zoom, size * zoom)))
                        .collect();
                    for link in &state.graph.links {
                        if let (Some(from), Some(to)) = (rects.get(&link.source), rects.get(&link.target)) {
                            let (a, b) = (from.right_center(), to.left_center());
                            let reach = ((b.x - a.x).abs() * 0.5).max(32.0 * zoom);
                            let ink = if selected.contains(&link.source) || selected.contains(&link.target) { t.form_primary } else { t.form_dim };
                            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                                [a, a + vec2(reach, 0.0), b - vec2(reach, 0.0), b],
                                false,
                                egui::Color32::TRANSPARENT,
                                Stroke::new(1.5, ink),
                            ));
                        }
                    }
                    for node in &state.graph.nodes {
                        let Some(rect) = rects.get(&node.id).copied() else { continue };
                        if !ui.clip_rect().intersects(rect) {
                            continue;
                        }
                        let response = ui.interact(rect, ui.id().with(("node", node.id)), Sense::click_and_drag());
                        crate::scenario::publish_handle(&format!("node_graph:node:{}", node.id), rect.center());
                        crate::scenario::publish_handle(&format!("node_graph:{}", node.name), rect.center());
                        let picked = selected.contains(&node.id);
                        let dimmed = node.suppressed || node.rolled_back;
                        let ink = if node.error.is_some() {
                            t.form_error
                        } else if picked {
                            t.form_primary
                        } else {
                            t.border
                        };
                        let fill = if node.error.is_some() { t.form_error_fill } else { t.panel };
                        painter.rect_filled(rect, t.control_radius, fill);
                        painter.rect_stroke(rect, t.control_radius, Stroke::new(if picked { 2.0 } else { 1.0 }, ink), egui::StrokeKind::Inside);
                        painter.rect_filled(Rect::from_min_size(rect.min, vec2(4.0 * zoom, rect.height())), t.control_radius, ink);
                        let text_ink = if dimmed { t.form_dim } else { t.form_ink };
                        let clipped = painter.with_clip_rect(rect.shrink(8.0 * zoom).intersect(ui.clip_rect()));
                        clipped.text(
                            rect.min + vec2(14.0, 14.0) * zoom,
                            Align2::LEFT_TOP,
                            &node.name,
                            FontId::proportional(t.font_body * zoom),
                            text_ink,
                        );
                        let badge = if node.suppressed {
                            i18n::tr("node_graph.suppressed")
                        } else if node.rolled_back {
                            i18n::tr("node_graph.rolled_back")
                        } else {
                            &node.kind
                        };
                        clipped.text(
                            rect.min + vec2(14.0, 37.0) * zoom,
                            Align2::LEFT_TOP,
                            badge,
                            FontId::proportional(t.font_small * zoom),
                            t.form_dim,
                        );
                        if let Some(parameter) = node.parameters.first() {
                            clipped.text(
                                rect.min + vec2(14.0, 60.0) * zoom,
                                Align2::LEFT_TOP,
                                parameter,
                                FontId::proportional(t.font_small * zoom),
                                text_ink,
                            );
                        }
                        for point in [rect.left_center(), rect.right_center()] {
                            painter.circle_filled(point, 4.0 * zoom, if picked { t.form_primary } else { t.form_dim });
                        }
                        if response.double_clicked() {
                            action = Some(Action::Edit(node.id));
                        } else if response.clicked() {
                            action = Some(Action::Pick(node.id));
                        }
                        if response.dragged()
                            && let Some(position) = state.positions.get_mut(&node.id)
                        {
                            let next = position.to_vec2() + response.drag_delta() / zoom;
                            *position = pos2(next.x.max(12.0), next.y.max(12.0));
                        }
                        response.on_hover_ui(|ui| {
                            ui.strong(&node.name);
                            for parameter in &node.parameters {
                                ui.label(parameter);
                            }
                            if let Some(error) = &node.error {
                                ui.colored_label(t.form_error, error);
                            }
                            if let Some(warning) = &node.warning {
                                ui.label(warning);
                            }
                        });
                    }
                });
        },
    );
    state.open &= !closed;
    match action {
        Some(Action::Pick(id)) => {
            let _ = app.run("select.set", json!({"items":[{"type":"feature","id":id}]}));
            app.tree.timeline_reveal = Some(id);
        }
        Some(Action::Edit(id)) => {
            state.open = false;
            app.edit_feature(id);
        }
        Some(Action::Suppress(id)) => {
            let _ = app.run_heavy("timeline.suppress", json!({"feature":id}));
        }
        None => {}
    }
    app.node_view = state;
}
