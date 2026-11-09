//! Stock connectors are fixed manufacturer models; the tool only places them.
use serde_json::{Map, Value, json};
use solidcraft_engine::stock_connectors::{self, Connector};

use crate::ui_kit::{choice_row, expr_row};
use crate::{SolveApp, dialogs::Kind, i18n, selection::SelInput};

#[derive(Clone, Debug)]
pub struct StockConnector {
    mount: Option<EndMount>,
    models: Vec<Connector>,
    selected: usize,
    origin: [String; 3],
    rotation: [String; 3],
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
            origin: ["0".into(), "0".into(), "0".into()],
            rotation: ["0 deg".into(), "0 deg".into(), "0 deg".into()],
        }),
        Vec::new(),
    ))
}

pub fn rows(ui: &mut egui::Ui, state: &mut StockConnector) -> bool {
    let mut enter = false;
    let names: Vec<_> = state.models.iter().map(|model| format!("{} · {} · {}", model.series, model.part_number, model.supplier)).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    choice_row(ui, i18n::tr("connector.model"), "aluminium_connector_model", &mut state.selected, &names);
    if let Some(model) = state.models.get(state.selected) {
        preview(ui, model);
        ui.label(i18n::format("connector.slot", &[("slot", &model.slot.to_string())]));
        ui.hyperlink_to(i18n::tr("connector.source"), &model.product_url);
    }
    if let Some(mount) = &mut state.mount {
        ui.label(i18n::format("aluminium.attached_to", &[("body", &mount.body)]));
        choice_row(ui, i18n::tr("aluminium.connector_side"), "aluminium_connector_side", &mut mount.side, &["1", "2", "3", "4"]);
        enter |= expr_row(ui, i18n::tr("aluminium.connector_offset"), &mut mount.gap);
        ui.add(egui::Label::new(i18n::tr("aluminium.connector_mount_note")).wrap());
        return enter;
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
    let model = state.models.get(state.selected).ok_or("choose a connector model")?;
    let mut params = json!({"model":model.id,"origin":state.origin,"rotation":state.rotation});
    if let Some(mount) = &state.mount {
        params["mount"] = json!({"body":mount.body,"end":mount.end,"side":mount.side,"gap":mount.gap});
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
}

pub(crate) fn at_port(app: &mut SolveApp, body: &str, end: solidcraft_engine::doc::aluminium::Port) {
    let Ok(info) = solidcraft_engine::aluminium_ports::member(&app.session, body) else { return };
    app.start("aluminium.connector");
    if let Some(Kind::AluminiumConnector(state)) = app.dialog.as_mut().map(|d| &mut d.kind) {
        state.models.retain(|m| f64::from(m.series) == info.frame.size);
        state.selected = 0;
        state.mount = Some(EndMount { body: body.to_owned(), end, side: 0, gap: "0 mm".into() });
    }
}

pub(crate) fn mount_guide(app: &SolveApp, ui: &egui::Ui, proj: &crate::viewport::Proj) {
    let Some(Kind::AluminiumConnector(state)) = app.dialog.as_ref().map(|d| &d.kind) else { return };
    let Some(mount) = &state.mount else { return };
    let Ok(info) = solidcraft_engine::aluminium_ports::member(&app.session, &mount.body) else { return };
    let frame = info.frame;
    let gap = app
        .session
        .doc
        .eval(&app.session.doc.with_design_unit(&mount.gap, solidcraft_engine::doc::expr::Kind::Length), solidcraft_engine::doc::expr::Kind::Length)
        .unwrap_or(0.0);
    let (at, n, _) = frame.port(mount.end, 0.5, gap);
    let r = match mount.side {
        0 => frame.x,
        1 => frame.y,
        2 => -frame.x,
        _ => -frame.y,
    };
    let corner = at + r * (frame.size / 2.0);
    let length = frame.size * 0.9;
    let stroke = egui::Stroke::new(3.0, crate::theme::Tokens::get().joint_manipulator);
    if let Some(c) = proj.to_screen(corner) {
        ui.painter().circle_stroke(c, 7.0, stroke);
        for p in [corner - n * length, corner + r * length] {
            if let Some(p) = proj.to_screen(p) {
                ui.painter().line_segment([c, p], stroke);
            }
        }
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

fn preview(ui: &mut egui::Ui, model: &Connector) {
    let memory = egui::Id::new("aluminium_connector_preview_cache");
    let mut cache = ui.data_mut(|data| data.remove_temp::<PreviewCache>(memory)).unwrap_or_default();
    if let Some((key, receiver)) = &cache.pending {
        let reply = receiver.lock().ok().and_then(|receiver| match receiver.try_recv() {
            Ok(reply) => Some(reply),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("connector preview worker ended".into())),
        });
        if let Some(reply) = reply {
            let texture = reply.map(|image| ui.ctx().load_texture(format!("connector_preview:{key}"), image, egui::TextureOptions::LINEAR));
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
            let context = ui.ctx().clone();
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
    let tokens = crate::theme::Tokens::get();
    ui.label(egui::RichText::new(i18n::tr("connector.preview")).small().color(tokens.form_dim));
    ui.vertical_centered(|ui| match cache.images.get(&key) {
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
                cache.images.remove(&key);
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
    if cache.pending.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    ui.data_mut(|data| data.insert_temp(memory, cache));
}
