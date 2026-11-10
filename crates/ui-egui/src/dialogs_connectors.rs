//! Stock connectors are fixed manufacturer models; the tool only places them.
#[path = "connector_browser.rs"]
mod browser;

use serde_json::{Map, Value, json};
use solidcraft_engine::stock_connectors::{self, Connector};

use crate::ui_kit::{choice_row, expr_row, text_row};
use crate::{SolveApp, dialogs::Kind, i18n, selection::SelInput};

#[derive(Clone, Debug)]
pub struct StockConnector {
    mount: Option<EndMount>,
    models: Vec<Connector>,
    selected: usize,
    filter: Filter,
    browser: browser::State,
    visible: bool,
    origin: [String; 3],
    rotation: [String; 3],
}

#[derive(Clone, Debug, Default)]
struct Filter {
    category: usize,
    search: String,
    series_filter: usize,
    slot_filter: usize,
    type_filter: usize,
    availability_filter: usize,
}

pub fn start(_: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    if id != "aluminium.connector" {
        return None;
    }
    Some((
        Kind::AluminiumConnector(StockConnector {
            mount: None,
            models: stock_connectors::catalogue().ok()?,
            selected: 0,
            filter: Filter::default(),
            browser: browser::State::initial(),
            visible: true,
            origin: ["0".into(), "0".into(), "0".into()],
            rotation: ["0 deg".into(), "0 deg".into(), "0 deg".into()],
        }),
        Vec::new(),
    ))
}

fn category_keys(state: &StockConnector) -> Vec<String> {
    let mut categories = vec!["all".to_owned()];
    for model in &state.models {
        if !categories.contains(&model.category) {
            categories.push(model.category.clone());
        }
    }
    categories
}

fn numeric_filters(state: &StockConnector) -> (Vec<u32>, Vec<u32>) {
    let mut series = vec![0];
    let mut slots = vec![0];
    for m in &state.models {
        for s in &m.compatible_series {
            if !series.contains(s) {
                series.push(*s);
            }
        }
        for s in &m.compatible_slots {
            if !slots.contains(s) {
                slots.push(*s);
            }
        }
    }
    series.sort_unstable();
    slots.sort_unstable();
    (series, slots)
}

fn filter_controls(ui: &mut egui::Ui, state: &mut StockConnector, category: bool) {
    let categories = category_keys(state);
    if state.filter.category >= categories.len() {
        state.filter.category = 0;
    }
    let keys: Vec<_> = categories.iter().map(|c| format!("connector.category.{c}")).collect();
    let labels: Vec<_> = keys.iter().map(|key| i18n::tr(key)).collect();
    if category {
        choice_row(ui, i18n::tr("connector.category"), "connector_category", &mut state.filter.category, &labels);
    }
    let _ = text_row(ui, i18n::tr("catalog.search"), &mut state.filter.search, i18n::tr("catalog.search_hint"));
    let (series, slots) = numeric_filters(state);
    let series_labels: Vec<_> = series.iter().map(|s| if *s == 0 { i18n::tr("catalog.any").to_owned() } else { s.to_string() }).collect();
    let series_labels: Vec<_> = series_labels.iter().map(String::as_str).collect();
    let slot_labels: Vec<_> = slots.iter().map(|s| if *s == 0 { i18n::tr("catalog.any").to_owned() } else { s.to_string() }).collect();
    let slot_labels: Vec<_> = slot_labels.iter().map(String::as_str).collect();
    let filters = ui.collapsing(i18n::tr("catalog.filters"), |ui| {
        choice_row(ui, i18n::tr("catalog.series"), "connector_series", &mut state.filter.series_filter, &series_labels);
        choice_row(ui, i18n::tr("catalog.slot"), "connector_slot", &mut state.filter.slot_filter, &slot_labels);
        choice_row(
            ui,
            i18n::tr("catalog.profile_type"),
            "connector_profile_type",
            &mut state.filter.type_filter,
            &[i18n::tr("catalog.any"), "I", "B", "O"],
        );
        choice_row(
            ui,
            i18n::tr("catalog.resources"),
            "connector_resources",
            &mut state.filter.availability_filter,
            &[i18n::tr("catalog.any"), i18n::tr("catalog.models"), i18n::tr("catalog.documents")],
        );
    });
    crate::scenario::publish_handle("connector:filters", filters.header_response.rect.center());
}

fn filtered(state: &StockConnector) -> Vec<usize> {
    let categories = category_keys(state);
    let (series, slots) = numeric_filters(state);
    let selected_series = series.get(state.filter.series_filter).copied().unwrap_or(0);
    let selected_slot = slots.get(state.filter.slot_filter).copied().unwrap_or(0);
    let selected_type = ["", "I", "B", "O"].get(state.filter.type_filter).copied().unwrap_or("");
    state
        .models
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            (state.filter.category == 0 || categories.get(state.filter.category).is_some_and(|category| &m.category == category))
                && state.filter.search.split_whitespace().all(|word| {
                    m.matches_query(word) || i18n::tr(&format!("connector.category.{}", m.category)).to_lowercase().contains(&word.to_lowercase())
                })
                && (selected_series == 0 || m.compatible_series.contains(&selected_series))
                && (selected_slot == 0 || m.compatible_slots.contains(&selected_slot))
                && (selected_type.is_empty() || m.profile_type.split('/').any(|t| t == selected_type))
                && match state.filter.availability_filter {
                    1 => m.insertable(),
                    2 => !m.insertable(),
                    _ => true,
                }
        })
        .map(|(i, _)| i)
        .collect()
}

pub fn rows(ui: &mut egui::Ui, state: &mut StockConnector) -> bool {
    let mut enter = false;
    let open = crate::ui_kit::button(ui, i18n::tr("browser.open"), crate::ui_kit::Button::Normal);
    crate::scenario::publish_handle("connector:browser", open.rect.center());
    if open.clicked() {
        browser::open(state);
    }
    if state.browser.open {
        browser::show(ui.ctx(), state);
        return false;
    }
    filter_controls(ui, state, true);
    let filtered = filtered(state);
    state.visible = !filtered.is_empty();
    let count = ui.label(i18n::format("catalog.count", &[("count", &filtered.len().to_string()), ("total", &state.models.len().to_string())]));
    crate::scenario::publish_handle(&format!("connector:results:{}", filtered.len()), count.rect.center());
    if !state.visible {
        ui.label(i18n::tr("catalog.no_matches"));
        return false;
    }
    let mut selected = filtered.iter().position(|i| *i == state.selected).unwrap_or(0);
    let names: Vec<_> = filtered
        .iter()
        .filter_map(|i| state.models.get(*i))
        .map(|m| {
            format!(
                "{} · {}",
                if m.compatible_series.is_empty() {
                    i18n::tr("catalog.unspecified").into()
                } else {
                    m.compatible_series.iter().map(u32::to_string).collect::<Vec<_>>().join("/")
                },
                m.part_number
            )
        })
        .collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    choice_row(ui, i18n::tr("connector.model"), "aluminium_connector_model", &mut selected, &names);
    if let Some(index) = filtered.get(selected) {
        state.selected = *index;
    }
    if let Some(model) = state.models.get(state.selected) {
        ui.label(&model.label);
        if matches!(model.geometry.as_str(), "surface" | "mixed") {
            ui.add(egui::Label::new(i18n::tr("catalog.surface_model")).wrap());
        }
        if model.insertable() {
            preview(ui, model);
        } else {
            ui.add(egui::Label::new(i18n::tr(if model.has_model() { "catalog.import_failed" } else { "catalog.documents_only" })).wrap());
        }
        if !model.compatible_slots.is_empty() {
            ui.label(i18n::format("connector.slot", &[("slot", &model.compatible_slots.iter().map(u32::to_string).collect::<Vec<_>>().join("/"))]));
        }
        if !model.profile_type.is_empty() {
            ui.label(format!("{}: {}", i18n::tr("catalog.profile_type"), model.profile_type));
        }
        ui.hyperlink_to(i18n::tr("connector.source"), &model.product_url);
        egui::CollapsingHeader::new(i18n::tr("catalog.source_file")).id_salt(&model.id).default_open(!model.insertable()).show(ui, |ui| {
            for (index, resource) in model.resources.iter().enumerate() {
                let response =
                    ui.hyperlink_to(format!("{} {} · {}", i18n::tr("catalog.source_file"), index + 1, resource.kind.to_uppercase()), &resource.url);
                crate::scenario::publish_handle(&format!("connector:resource:{index}"), response.rect.center());
            }
        });
        if !model.insertable() {
            return false;
        }
        if state.mount.is_some() && model.placement == "manual" {
            ui.add(egui::Label::new(i18n::tr("catalog.manual_mount")).wrap());
        }
    }
    if let Some(mount) = &mut state.mount {
        ui.label(i18n::format("aluminium.attached_to", &[("body", &mount.body)]));
        if mount.end.is_end() {
            choice_row(ui, i18n::tr("aluminium.connector_side"), "aluminium_connector_side", &mut mount.side, &["1", "2", "3", "4"]);
        }
        enter |= expr_row(ui, i18n::tr("aluminium.connector_offset"), &mut mount.gap);
        ui.add(egui::Label::new(i18n::tr("interface.placement_note")).wrap());
    }
    ui.label(i18n::tr("connector.position"));
    for (label, value) in ["X", "Y", "Z"].into_iter().zip(&mut state.origin) {
        enter |= expr_row(ui, label, value);
    }
    ui.label(i18n::tr("connector.rotation"));
    for (label, value) in ["X", "Y", "Z"].into_iter().zip(&mut state.rotation) {
        enter |= expr_row(ui, label, value);
    }
    ui.label(egui::RichText::new(i18n::tr("connector.fixed_note")).small().color(crate::theme::Tokens::get().form_dim));
    enter
}

pub fn commands(app: &SolveApp, state: &StockConnector, extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    if state.browser.open {
        return Err(i18n::tr("browser.choose_hint").into());
    }
    if !state.visible {
        return Err(i18n::tr("catalog.no_matches").into());
    }
    let model = state.models.get(state.selected).ok_or("choose a connector model")?;
    if !model.insertable() {
        return Err(i18n::tr(if model.has_model() { "catalog.import_failed" } else { "catalog.documents_only" }).into());
    }
    let mut params = json!({"model":model.id,"origin":state.origin,"rotation":state.rotation});
    if let Some(mount) = &state.mount {
        params["mount"] =
            json!({"body":mount.body,"port":mount.end,"side":mount.side,"gap":mount.gap,"interface":mount.interface,"position":mount.position});
    }
    if let Some(name) = extra.get("name") {
        params["name"] = name.clone();
    }
    let params = stock_connectors::import_params(&app.session, &params).map_err(|error| error.to_string())?;
    Ok(vec![("file.insert_step".into(), params)])
}

#[derive(Clone, Debug)]
struct EndMount {
    body: String,
    end: solidcraft_engine::doc::aluminium::Port,
    side: usize,
    gap: String,
    interface: String,
    position: f64,
}

pub(crate) fn at_interface(app: &mut SolveApp, interface: &crate::aluminium_handles::Interface) {
    let Ok(info) = solidcraft_engine::aluminium_ports::member(&app.session, &interface.body) else { return };
    app.start("aluminium.connector");
    if let Some(Kind::AluminiumConnector(state)) = app.dialog.as_mut().map(|d| &mut d.kind) {
        state.models.retain(|m| m.compatible(info.frame.size, interface.kind));
        state.models.sort_by_key(|m| {
            if interface.kind == "slot" {
                match m.category.as_str() {
                    "nut" => 0,
                    "internal" => 1,
                    _ => 2,
                }
            } else {
                0
            }
        });
        state.selected = 0;
        state.filter.category = 0;
        state.mount = Some(EndMount {
            body: interface.body.clone(),
            end: interface.port,
            side: 0,
            gap: "0 mm".into(),
            interface: interface.kind.into(),
            position: interface.position,
        });
    }
}

pub(crate) fn mount_guide(app: &SolveApp, ui: &egui::Ui, proj: &crate::viewport::Proj) {
    let Some(Kind::AluminiumConnector(state)) = app.dialog.as_ref().map(|d| &d.kind) else { return };
    let Some(mount) = &state.mount else { return };
    let Ok(info) = solidcraft_engine::aluminium_ports::member(&app.session, &mount.body) else { return };
    let (at, _, _) = info.frame.port(mount.end, mount.position, 0.0);
    if let Some(c) = proj.to_screen(at) {
        ui.painter().circle_stroke(c, 7.0, egui::Stroke::new(2.0, crate::theme::Tokens::get().joint_manipulator));
        crate::scenario::publish_handle("aluminium:connector_mount", c);
    }
}

// Preview work never touches the active document. Only one catalogue image is computed at
// a time; a late result is cached under its own model and cannot replace the selected one.
type ImageReply = Result<egui::ColorImage, String>;
type ImageReceiver = std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<ImageReply>>>;

#[derive(Clone, Default)]
struct PreviewCache {
    images: std::collections::BTreeMap<String, Result<egui::TextureHandle, String>>,
    pending: Option<(String, ImageReceiver)>,
    used: std::collections::VecDeque<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn picture(model: &Connector) -> ImageReply {
    use solidcraft_engine::render::{Camera, StandardView};
    let mut session = solidcraft_engine::Session::default();
    session.execute("aluminium.connector", &json!({"model":model.id})).map_err(|error| error.to_string())?;
    let mut camera = Camera::default();
    camera.set_view(StandardView::Iso);
    // These stock models have their open corner towards -X/+Y.
    camera.yaw += std::f64::consts::PI;
    let bounds = solidcraft_engine::view::bounds(&session);
    if bounds.is_empty() {
        return Err("the connector has no visible geometry".into());
    }
    camera.fit(&bounds);
    camera.distance *= 0.78;
    let scene = solidcraft_engine::view::scene(&session, &camera);
    let image = solidcraft_engine::capture::rgba(&scene, &camera, 420, 280)?;
    Ok(egui::ColorImage::from_rgba_unmultiplied([420, 280], &image.rgba))
}

fn thumbnail(ctx: &egui::Context, model: &Connector) -> Option<Result<egui::TextureHandle, String>> {
    let memory = egui::Id::new("aluminium_connector_preview_cache");
    let mut cache = ctx.data_mut(|data| data.remove_temp::<PreviewCache>(memory)).unwrap_or_default();
    if let Some((key, receiver)) = &cache.pending {
        let reply = receiver.lock().ok().and_then(|receiver| match receiver.try_recv() {
            Ok(reply) => Some(reply),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("connector preview worker ended".into())),
        });
        if let Some(reply) = reply {
            let texture = reply.map(|image| ctx.load_texture(format!("connector_preview:{key}"), image, egui::TextureOptions::LINEAR));
            if cache.images.len() >= 48 {
                let discard = cache.used.pop_front().or_else(|| cache.images.keys().find(|k| *k != key).cloned());
                if let Some(discard) = discard {
                    cache.images.remove(&discard);
                }
            }
            cache.images.insert(key.clone(), texture);
            cache.pending = None;
        }
    }
    let key = format!("{}:{}", model.id, model.step_sha256);
    if !cache.images.contains_key(&key) && cache.pending.is_none() {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            let model = model.clone();
            let context = ctx.clone();
            match std::thread::Builder::new().name("connector preview".into()).spawn(move || {
                let _ = sender.send(picture(&model));
                context.request_repaint();
            }) {
                Ok(_) => cache.pending = Some((key.clone(), std::sync::Arc::new(std::sync::Mutex::new(receiver)))),
                Err(error) => {
                    cache.images.insert(key.clone(), Err(error.to_string()));
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        cache.images.insert(key.clone(), Err("connector pictures require a native renderer".into()));
    }
    let result = cache.images.get(&key).cloned();
    if result.is_some() {
        cache.used.retain(|k| k != &key);
        cache.used.push_back(key);
    }
    if cache.pending.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    ctx.data_mut(|data| data.insert_temp(memory, cache));
    result
}

fn forget_thumbnail(ctx: &egui::Context, model: &Connector) {
    ctx.data_mut(|data| {
        let key = format!("{}:{}", model.id, model.step_sha256);
        let cache = data.get_temp_mut_or_default::<PreviewCache>(egui::Id::new("aluminium_connector_preview_cache"));
        cache.images.remove(&key);
        cache.used.retain(|k| k != &key);
    });
}

fn preview(ui: &mut egui::Ui, model: &Connector) {
    let picture = thumbnail(ui.ctx(), model);
    let tokens = crate::theme::Tokens::get();
    ui.label(egui::RichText::new(i18n::tr("connector.preview")).small().color(tokens.form_dim));
    ui.vertical_centered(|ui| match picture.as_ref() {
        Some(Ok(texture)) => {
            let width = ui.available_width().min(240.0);
            let response = ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(width, width * 2.0 / 3.0))
                    .corner_radius(tokens.control_radius)
                    .alt_text(i18n::format("connector.preview_alt", &[("part", &model.part_number)])),
            );
            crate::scenario::publish_handle(&format!("connector:preview:{}", model.part_number), response.rect.center());
            response.on_hover_ui(|ui| {
                ui.label(&model.part_number);
                ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(360.0, 240.0)));
            });
        }
        Some(Err(_)) => {
            ui.label(i18n::tr("connector.preview_failed"));
            if crate::ui_kit::button(ui, i18n::tr("connector.preview_retry"), crate::ui_kit::Button::Small).clicked() {
                forget_thumbnail(ui.ctx(), model);
                ui.ctx().request_repaint();
            }
        }
        None => {
            ui.allocate_ui(egui::vec2(ui.available_width(), 128.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(i18n::tr("connector.preview_loading"));
                    });
                });
            });
        }
    });
}
