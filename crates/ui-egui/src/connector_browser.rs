//! Visual catalogue picker. Browsing changes only draft UI state, never the design.
use super::{Filter, StockConnector, category_keys, filter_controls, filtered, preview, thumbnail};
use crate::{i18n, theme::Tokens, ui_kit};
use egui::{Align2, FontId, Id, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, vec2};
use solidcraft_engine::stock_connectors::Connector;

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub open: bool,
    focused: Option<usize>,
    before: Option<Filter>,
}

impl State {
    pub fn initial() -> Self {
        Self { open: true, focused: Some(0), before: Some(Filter::default()) }
    }
}

pub(super) fn open(state: &mut StockConnector) {
    state.browser = State { open: true, focused: Some(state.selected), before: Some(state.filter.clone()) };
}

fn close(state: &mut StockConnector, accept: bool) {
    if accept {
        if let Some(index) = state.browser.focused.filter(|i| filtered(state).contains(i))
            && state.models.get(index).is_some_and(Connector::insertable)
        {
            state.selected = index;
            state.visible = true;
        } else {
            return;
        }
    } else if let Some(before) = state.browser.before.take() {
        state.filter = before;
    }
    state.browser.open = false;
}

pub(super) fn show(ctx: &egui::Context, state: &mut StockConnector) {
    let tk = Tokens::get();
    let escape = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let enter = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let width = (ctx.content_rect().width() - tk.space_24 * 2.0).clamp(260.0, 1280.0);
    let mut accept = false;
    let mut dismiss = false;
    let closed = ui_kit::Window::new(Id::new("stock_part_browser"), "import", i18n::tr("browser.title"))
        .cards(width / tk.form_card_width)
        .keep_open_on_escape()
        .show_modal_body(ctx, |ui| {
            crate::scenario::publish_handle("connector_browser:open", ui.min_rect().min);
            let sidebar = ui.available_width() >= 900.0;
            filter_controls(ui, state, !sidebar);
            ui.horizontal(|ui| {
                if ui_kit::button(ui, i18n::tr("browser.reset"), ui_kit::Button::Small).clicked() {
                    state.filter = Filter::default();
                }
                ui.label(i18n::tr(if state.mount.is_some() { "browser.matched_scope" } else { "browser.all_scope" }));
            });
            let matches = filtered(state);
            if !state.browser.focused.is_some_and(|i| matches.contains(&i)) {
                state.browser.focused = matches.first().copied();
            }
            let count = ui.label(i18n::format("catalog.count", &[("count", &matches.len().to_string()), ("total", &state.models.len().to_string())]));
            crate::scenario::publish_handle(&format!("connector_browser:results:{}", matches.len()), count.rect.center());
            // The selected part gets the first thumbnail job; visible tiles fill the same bounded cache.
            if let Some(model) = state.browser.focused.and_then(|i| state.models.get(i)).filter(|m| m.insertable()) {
                let _ = thumbnail(ctx, model);
            }
            let height = (ctx.content_rect().height() - 360.0).clamp(180.0, 520.0);
            let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
            let gap = tk.space_12;
            let side_width = if sidebar { 170.0 } else { 0.0 };
            let detail_width = if area.width() >= 680.0 { 280.0 } else { 0.0 };
            let grid_left = area.left() + if sidebar { side_width + gap } else { 0.0 };
            let grid_right = area.right() - if detail_width > 0.0 { detail_width + gap } else { 0.0 };
            if sidebar {
                let rect = Rect::from_min_size(area.min, vec2(side_width, height));
                column(ui, rect, "part_categories", |ui| categories(ui, state));
            }
            let grid = Rect::from_min_max(Pos2::new(grid_left, area.top()), Pos2::new(grid_right.max(grid_left), area.bottom()));
            column(ui, grid, "part_grid", |ui| {
                if matches.is_empty() {
                    ui.label(i18n::tr("catalog.no_matches"));
                } else {
                    accept |= cards(ui, state, &matches, height);
                }
            });
            if detail_width > 0.0 {
                let rect = Rect::from_min_size(Pos2::new(area.right() - detail_width, area.top()), vec2(detail_width, height));
                column(ui, rect, "part_detail", |ui| {
                    egui::ScrollArea::vertical().id_salt("part_detail_scroll").max_height(height).show(ui, |ui| detail(ui, state));
                });
            } else {
                ui.collapsing(i18n::tr("connector.preview"), |ui| detail(ui, state));
            }
            ui.advance_cursor_after_rect(area);
            ui.separator();
            ui.label(egui::RichText::new(i18n::tr("browser.instructions")).small().color(tk.text_dim));
            let usable = state.browser.focused.is_some_and(|i| matches.contains(&i) && state.models.get(i).is_some_and(Connector::insertable));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let choose = ui_kit::button(ui, i18n::tr("browser.use"), ui_kit::Button::Enabled(usable));
                crate::scenario::publish_handle("connector_browser:use", choose.rect.center());
                accept |= choose.clicked();
                let cancel = ui_kit::button(ui, i18n::tr("browser.back"), ui_kit::Button::Normal);
                crate::scenario::publish_handle("connector_browser:back", cancel.rect.center());
                dismiss |= cancel.clicked();
            });
            accept |= enter && usable;
        });
    if closed || dismiss || escape {
        close(state, false);
    } else if accept {
        close(state, true);
    }
}

fn column(ui: &mut Ui, rect: Rect, id: &str, content: impl FnOnce(&mut Ui)) {
    ui.scope_builder(UiBuilder::new().id_salt(id).max_rect(rect).layout(Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(rect));
        ui.set_width(rect.width());
        content(ui);
    });
}

fn categories(ui: &mut Ui, state: &mut StockConnector) {
    ui.label(egui::RichText::new(i18n::tr("connector.category")).strong());
    egui::ScrollArea::vertical().id_salt("part_category_scroll").show(ui, |ui| {
        for (index, category) in category_keys(state).iter().enumerate() {
            let count = state.models.iter().filter(|m| category == "all" || &m.category == category).count();
            let text = format!("{}  {count}", i18n::tr(&format!("connector.category.{category}")));
            let response = ui.selectable_label(state.filter.category == index, text);
            crate::scenario::publish_handle(&format!("connector_browser:category:{category}"), response.rect.center());
            if response.clicked() {
                state.filter.category = index;
            }
        }
    });
}

fn short(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit { text.to_owned() } else { format!("{}…", text.chars().take(limit.saturating_sub(1)).collect::<String>()) }
}

fn cards(ui: &mut Ui, state: &mut StockConnector, matches: &[usize], height: f32) -> bool {
    let tk = Tokens::get();
    let gap = tk.space_8;
    let columns = (((ui.available_width() + gap) / 160.0).floor() as usize).clamp(1, 8);
    let width = ((ui.available_width() - gap * columns.saturating_sub(1) as f32) / columns as f32).max(40.0);
    let tile_height = width * 2.0 / 3.0 + 70.0;
    let mut accept = false;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt((
            "part_grid_scroll",
            &state.filter.search,
            state.filter.category,
            state.filter.series_filter,
            state.filter.slot_filter,
            state.filter.type_filter,
            state.filter.availability_filter,
        ))
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show_rows(ui, tile_height + gap, matches.len().div_ceil(columns), |ui, rows| {
            for row in rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for col in 0..columns {
                        let Some(index) = matches.get(row * columns + col).copied() else { break };
                        let Some(model) = state.models.get(index) else { continue };
                        let (rect, response) = ui.allocate_exact_size(vec2(width, tile_height), Sense::click());
                        let selected = state.browser.focused == Some(index);
                        let fill = if selected {
                            tk.accent_soft
                        } else if response.hovered() {
                            tk.hover
                        } else {
                            tk.panel_header
                        };
                        ui.painter().rect(
                            rect,
                            tk.control_radius,
                            fill,
                            Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { tk.accent } else { tk.border }),
                            StrokeKind::Inside,
                        );
                        let picture = Rect::from_min_size(
                            rect.min + vec2(gap, gap),
                            vec2((width - gap * 2.0).max(1.0), (width - gap * 2.0).max(1.0) * 2.0 / 3.0),
                        );
                        if model.insertable() {
                            match thumbnail(ui.ctx(), model) {
                                Some(Ok(texture)) => {
                                    egui::Image::new(&texture).corner_radius(tk.control_radius).paint_at(ui, picture);
                                    crate::scenario::publish_handle(&format!("connector_browser:image:{}", model.id), picture.center());
                                }
                                Some(Err(_)) => crate::icons::paint_icon(ui.painter(), picture.shrink(20.0), "import", 1.0),
                                None => {
                                    ui.painter().text(
                                        picture.center(),
                                        Align2::CENTER_CENTER,
                                        "…",
                                        FontId::proportional(tk.font_heading),
                                        tk.text_dim,
                                    );
                                }
                            }
                        } else {
                            crate::icons::paint_icon(ui.painter(), picture.shrink(20.0), "inspect", 1.0);
                        }
                        let painter = ui.painter().with_clip_rect(rect.shrink(4.0));
                        painter.text(
                            Pos2::new(rect.left() + gap, picture.bottom() + 10.0),
                            Align2::LEFT_TOP,
                            short(&model.part_number, (width / 7.0) as usize),
                            FontId::proportional(tk.font_caption),
                            tk.text,
                        );
                        let meta = if model.insertable() {
                            format!("{} · {}", model.compatible_series.iter().map(u32::to_string).collect::<Vec<_>>().join("/"), model.profile_type)
                        } else {
                            i18n::tr("catalog.documents").to_owned()
                        };
                        painter.text(
                            Pos2::new(rect.left() + gap, picture.bottom() + 32.0),
                            Align2::LEFT_TOP,
                            meta,
                            FontId::proportional(tk.font_small),
                            tk.text_dim,
                        );
                        crate::scenario::publish_handle(&format!("connector_browser:tile:{}", model.id), rect.center());
                        let double = response.double_clicked();
                        let clicked = response.on_hover_text(format!("{}\n{}", model.part_number, model.label)).clicked();
                        if clicked || double {
                            state.browser.focused = Some(index);
                        }
                        if double && model.insertable() {
                            accept = true;
                        }
                    }
                });
                ui.add_space(gap);
            }
        });
    accept
}

fn detail(ui: &mut Ui, state: &StockConnector) {
    let Some(model) = state.browser.focused.and_then(|i| state.models.get(i)) else { return };
    ui.heading(&model.part_number);
    ui.label(&model.label);
    if model.insertable() {
        preview(ui, model);
        ui.label(i18n::tr(if model.placement == "manual" { "browser.datum_unset" } else { "browser.datum_preset" }));
        if matches!(model.geometry.as_str(), "surface" | "mixed") {
            ui.label(i18n::tr("catalog.surface_model"));
        }
    } else {
        ui.label(i18n::tr("catalog.documents_only"));
    }
    let ports = solidcraft_engine::connector_ports::definitions(&model.id);
    if !ports.is_empty() {
        ui.label(i18n::format("ports.count", &[("count", &ports.len().to_string())]));
    }
    ui.separator();
    if !model.compatible_series.is_empty() {
        ui.label(format!("{}: {}", i18n::tr("catalog.series"), model.compatible_series.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
    }
    if !model.compatible_slots.is_empty() {
        ui.label(format!("{}: {}", i18n::tr("catalog.slot"), model.compatible_slots.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
    }
    if !model.profile_type.is_empty() {
        ui.label(format!("{}: {}", i18n::tr("catalog.profile_type"), model.profile_type));
    }
    ui.hyperlink_to(i18n::tr("connector.source"), &model.product_url);
    for (index, source) in model.resources.iter().enumerate() {
        let response = ui.hyperlink_to(format!("{} {} · {}", i18n::tr("catalog.source_file"), index + 1, source.kind.to_uppercase()), &source.url);
        crate::scenario::publish_handle(&format!("connector_browser:resource:{index}"), response.rect.center());
    }
    crate::scenario::publish_handle(&format!("connector_browser:detail:{}", model.id), ui.min_rect().center());
}
