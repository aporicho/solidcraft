//! Standalone procedural European-series aluminium extrusion.
use serde_json::{Value, json};
use solidcraft_doc::{Dir, FeatureKind, expr::Kind};
use solidcraft_geom::Vec3;

use super::CommandSpec;
use crate::params::{bad, expr, req_expr};
use crate::schema::features::{AluminiumProfileParams, FeatureResult};
use crate::{Result, Session};

const CONTRACT: crate::schema::TypedContract<AluminiumProfileParams> = crate::schema::TypedContract::new();
const CONNECTOR_CONTRACT: crate::schema::TypedContract<crate::schema::features::AluminiumConnectorParams> = crate::schema::TypedContract::new();
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("aluminium.connector", "Insert Connector", crate::checked!("aluminium.connector", connector))
        .at("ALUMINIUM", "CONNECT").icon("joint").enabled(super::not_in_sketch)
        .contract(&CONNECTOR_CONTRACT).output(crate::schema::output_schema_for::<Value>)
        .params("model?: motedis-s6bbr20|motedis-s8bbr30|motedis-s8ibr40; origin?: [x,y,z]; rotation?: [x,y,z] angle expressions (bare numbers in degrees); name? → imported component, occurrence, feature and part metadata. Uses the fixed manufacturer STEP model at original scale from the installed connector data cache; no network requests"),
    CommandSpec::new("aluminium.profile", "Create Aluminium Profile", crate::checked!("aluminium.profile", create))
        .at("ALUMINIUM", "CREATE").icon("frame_member").enabled(super::not_in_sketch)
        .contract(&CONTRACT).output(crate::schema::output_schema_for::<FeatureResult>)
        .params("model?: 2020|3030|4040 (default 2020); length: length expression; origin?: [x,y,z]; axis?: X|Y|Z (default Z); angle?: angle expression; name?, body_name?. Creates one editable true solid with four open T-slots, diagonal webs and a through bore. Section dimensions are fixed by model and cannot be edited"),
];

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "aluminium.profile";
    for key in ["size", "width", "height", "slot_width", "bore_diameter", "wall", "corner_radius", "series"] {
        if p.get(key).is_some() {
            return Err(bad(cmd, "the section is fixed by model; only length and placement can be adjusted"));
        }
    }
    let model = p.get("model").and_then(Value::as_str).unwrap_or("2020").to_owned();
    solidcraft_doc::aluminium::stock_section(&model).map_err(|e| bad(cmd, e.to_string()))?;
    let length = req_expr(cmd, p, "length")?;
    let angle = expr(p, "angle").unwrap_or_else(|| "0 deg".into());
    let eval = |e: &str| s.doc.eval(&s.doc.with_design_unit(e, Kind::Length), Kind::Length).map_err(|e| bad(cmd, e.to_string()));
    if !(0.1..=100_000.0).contains(&eval(&length)?) {
        return Err(bad(cmd, "length must be 0.1–100000 mm"));
    }
    super::features::check_expr(s, &angle, Kind::Angle, cmd, "angle")?;
    let direction = |value: &Value| -> Option<Vec3> {
        match value.as_str() {
            Some("X") => Some(Vec3::X),
            Some("Y") => Some(Vec3::Y),
            Some("Z") => Some(Vec3::Z),
            Some(_) => None,
            None => crate::params::vec3(value).and_then(|v| v.normalized()),
        }
    };
    let axis = p.get("axis").map_or(Some(Vec3::Z), direction).ok_or_else(|| bad(cmd, "axis must be X, Y, Z or a non-zero vector"))?;
    let reference = match p.get("reference") {
        Some(value) => direction(value).ok_or_else(|| bad(cmd, "invalid section reference direction"))?,
        None => solidcraft_doc::frame::member_axes(axis, 0.0).map(|v| v.0).ok_or_else(|| bad(cmd, "invalid axis"))?,
    };
    if axis.cross(reference).len() < 1e-8 {
        return Err(bad(cmd, "section reference cannot be parallel to the axis"));
    }
    let origin = super::features::point_param(s, p, "origin", cmd)?.unwrap_or_else(|| solidcraft_doc::point_expr(Vec3::ZERO));
    let mut params = p.clone();
    if params.get("body_name").is_none() {
        params["body_name"] = json!(format!("EU{model}"));
    }
    super::features::add_feature(
        s,
        &params,
        FeatureKind::AluminiumProfile { model, length, origin, axis: Dir(axis), reference: Dir(reference), angle },
    )
}

fn connector(s: &mut Session, p: &Value) -> Result<Value> {
    let params = crate::stock_connectors::import_params(s, p)?;
    super::file::insert_step(s, &params)
}
