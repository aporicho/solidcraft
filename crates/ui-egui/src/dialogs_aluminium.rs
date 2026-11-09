//! Stock aluminium profiles: select a fixed section, then set length and placement.
use serde_json::{Map, Value, json};
use solidcraft_engine::doc::{FeatureKind, Mat, apply_vector, map_point_expr};
use solidcraft_engine::geom::Vec3;

use crate::ui_kit::{choice_row, expr_row};
use crate::{SolveApp, dialogs::Kind, i18n, selection::SelInput};

#[derive(Clone, Debug)]
pub struct Profile {
    pub(crate) attachment: Option<Value>,
    model: usize,
    pub(crate) length: String,
    angle: String,
    axis: usize,
    original_axis: Vec3,
    reference: Option<Vec3>,
    origin: [String; 3],
}

impl Profile {
    fn new() -> Self {
        Self {
            attachment: None,
            model: 0,
            length: "300 mm".into(),
            angle: "0 deg".into(),
            axis: 2,
            original_axis: Vec3::Z,
            reference: None,
            origin: ["0".into(), "0".into(), "0".into()],
        }
    }
}

pub fn start(_: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    match id {
        "aluminium.profile" => Some((Kind::Aluminium(Profile::new()), Vec::new())),
        "aluminium.frame" => Some((Kind::AluminiumFrame(Skeleton { width: "600 mm".into(), height: "400 mm".into(), plane: 0 }), Vec::new())),
        _ => None,
    }
}

pub fn rows(app: &SolveApp, ui: &mut egui::Ui, m: &mut Profile) -> bool {
    let mut enter = false;
    choice_row(
        ui,
        i18n::tr("aluminium.model"),
        "aluminium_model",
        &mut m.model,
        &[i18n::tr("aluminium.model2020"), i18n::tr("aluminium.model3030"), i18n::tr("aluminium.model4040")],
    );
    enter |= expr_row(ui, i18n::tr("aluminium.length"), &mut m.length);
    if m.attachment.is_some()
        && crate::ui_kit::button(ui, i18n::tr("aluminium.detach"), crate::ui_kit::Button::Normal).clicked()
        && let Ok(commands) = commands(m, &Map::new())
        && let Some((_, params)) = commands.first()
        && let Ok((base, axis, reference)) = solidcraft_engine::aluminium_ports::placement(&app.session, params)
    {
        m.origin = [base.x, base.y, base.z].map(|v| format!("{v:.8} mm"));
        m.axis = 3;
        m.original_axis = axis;
        m.reference = Some(reference);
        m.attachment = None;
    }
    if let Some(mount) = &mut m.attachment {
        let body = mount.get("body").and_then(Value::as_str).unwrap_or_default();
        ui.label(i18n::format("aluminium.attached_to", &[("body", body)]));
        let mut position = mount.get("position").and_then(Value::as_str).unwrap_or("0.5").to_owned();
        let mut gap = mount.get("gap").and_then(Value::as_str).unwrap_or("0 mm").to_owned();
        ui.collapsing(i18n::tr("aluminium.precise_position"), |ui| {
            enter |= expr_row(ui, i18n::tr("aluminium.port_position"), &mut position);
        });
        enter |= expr_row(ui, i18n::tr("aluminium.port_gap"), &mut gap);
        mount["position"] = json!(position);
        mount["gap"] = json!(gap);
        enter |= expr_row(ui, i18n::tr("aluminium.rotation"), &mut m.angle);
        ui.add(egui::Label::new(i18n::tr("aluminium.attach_note")).wrap());
        return enter;
    }
    let old_axis = m.axis;
    let labels = ["X", "Y", "Z", i18n::tr("aluminium.keep_direction")];
    choice_row(ui, i18n::tr("aluminium.axis"), "aluminium_axis", &mut m.axis, &labels[..if old_axis == 3 { 4 } else { 3 }]);
    if old_axis != m.axis {
        m.reference = None;
    }
    enter |= expr_row(ui, i18n::tr("aluminium.rotation"), &mut m.angle);
    ui.collapsing(i18n::tr("aluminium.position"), |ui| {
        for (label, value) in ["X", "Y", "Z"].into_iter().zip(&mut m.origin) {
            enter |= expr_row(ui, label, value);
        }
    });
    ui.label(egui::RichText::new(i18n::tr("aluminium.stock_note")).small().color(crate::theme::Tokens::get().form_dim));
    enter
}

pub fn commands(m: &Profile, extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let model = solidcraft_engine::doc::aluminium::MODELS.get(m.model).ok_or("choose a profile model")?.0;
    let axis = match m.axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        2 => Vec3::Z,
        _ => m.original_axis,
    };
    let mut params = json!({
        "model":model, "length":m.length, "origin":m.origin,
        "axis":[axis.x,axis.y,axis.z], "angle":m.angle
    });
    if let Some(mount) = &m.attachment {
        params["attachment"] = mount.clone();
    }
    if let Some(reference) = m.reference {
        params["reference"] = json!([reference.x, reference.y, reference.z]);
    }
    for key in ["name", "body_name", "body_names"] {
        if let Some(value) = extra.get(key) {
            params[key] = value.clone();
        }
    }
    Ok(vec![("aluminium.profile".into(), params)])
}

pub fn for_feature(_: &SolveApp, kind: &FeatureKind, frame: &Mat) -> Option<(Kind, Vec<SelInput>, Map<String, Value>)> {
    let FeatureKind::AluminiumProfile { model, length, origin, axis, reference, angle, attachment } = kind else { return None };
    let axis = apply_vector(frame, axis.0);
    let mut origin = origin.clone();
    map_point_expr(frame, &mut origin);
    let m = Profile {
        attachment: attachment.as_ref().and_then(|a| serde_json::to_value(a).ok()),
        model: solidcraft_engine::doc::aluminium::MODELS.iter().position(|entry| entry.0 == model)?,
        length: length.to_string(),
        angle: angle.to_string(),
        axis: [Vec3::X, Vec3::Y, Vec3::Z].iter().position(|d| d.dist(axis) < 1e-8).unwrap_or(3),
        original_axis: axis,
        reference: Some(apply_vector(frame, reference.0)),
        origin: origin.map(|v| v.to_string()),
    };
    Some((Kind::Aluminium(m), Vec::new(), Map::new()))
}

/// Expanded into a standard sketch with native driving dimensions.
#[derive(Clone, Debug)]
pub struct Skeleton {
    width: String,
    height: String,
    plane: usize,
}

pub fn skeleton_rows(ui: &mut egui::Ui, m: &mut Skeleton) -> bool {
    let mut enter = expr_row(ui, i18n::tr("aluminium.frame_width"), &mut m.width);
    enter |= expr_row(ui, i18n::tr("aluminium.frame_height"), &mut m.height);
    choice_row(ui, i18n::tr("aluminium.frame_plane"), "aluminium_frame_plane", &mut m.plane, &["XY", "XZ", "YZ"]);
    ui.add(egui::Label::new(egui::RichText::new(i18n::tr("aluminium.skeleton_note")).small().color(crate::theme::Tokens::get().form_dim)).wrap());
    enter
}

pub fn skeleton_commands(m: &Skeleton) -> Result<Vec<(String, Value)>, String> {
    let plane = ["XY", "XZ", "YZ"].get(m.plane).ok_or("choose a skeleton plane")?;
    Ok(vec![
        ("aluminium.frame".into(), json!({"width":m.width,"height":m.height,"plane":plane,"name":i18n::tr("aluminium.skeleton_name")})),
        ("view.fit".into(), json!({})),
    ])
}

pub(crate) fn attach(app: &mut SolveApp, body: &str, port: solidcraft_engine::doc::aluminium::Port, position: f64) {
    let Ok(info) = solidcraft_engine::aluminium_ports::member(&app.session, body) else { return };
    app.start("aluminium.profile");
    if let Some(Kind::Aluminium(m)) = app.dialog.as_mut().map(|d| &mut d.kind) {
        m.model = solidcraft_engine::doc::aluminium::MODELS.iter().position(|e| e.0 == info.frame.model).unwrap_or(0);
        m.length = "100 mm".into();
        m.attachment = Some(json!({"body":body,"port":port,"position":position.to_string(),"gap":"0 mm"}));
    }
}
