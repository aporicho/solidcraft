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
const MEMBERS_CONTRACT: crate::schema::TypedContract<crate::schema::features::AluminiumMembersParams> = crate::schema::TypedContract::new();
const FRAME_CONTRACT: crate::schema::TypedContract<crate::schema::features::AluminiumFrameParams> = crate::schema::TypedContract::new();
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("aluminium.connector", "Insert Connector", crate::checked!("aluminium.connector", connector))
        .at("ALUMINIUM", "CONNECT").icon("joint").enabled(super::not_in_sketch)
        .contract(&CONNECTOR_CONTRACT).output(crate::schema::output_schema_for::<Value>)
        .params("mount?: {body: stock member, end: start|end, side?: 0–3, gap?: length} (preposition at an end side slot; inserted part is independent); model?: motedis-s6bbr20|motedis-s8bbr30|motedis-s8ibr40; origin?: [x,y,z]; rotation?: [x,y,z] angle expressions (bare numbers in degrees); name? → imported component, occurrence, feature and part metadata. Uses the fixed manufacturer STEP model at original scale from the installed connector data cache; no network requests"),
    CommandSpec::new("aluminium.frame", "Rectangular Skeleton", crate::checked!("aluminium.frame", skeleton))
        .at("ALUMINIUM", "CREATE").icon("rect").enabled(super::not_in_sketch)
        .contract(&FRAME_CONTRACT).output(crate::schema::output_schema_for::<Value>)
        .params("width, height: length expressions, 0.1–100000 mm; plane?: XY|XZ|YZ; name?. Creates a dimensioned editable rectangular sketch and selects its four lines. Dimensions describe member centre lines, not finished outside sizes. Use aluminium.members next; joints are not automatically trimmed"),
    CommandSpec::new("aluminium.members", "Profiles Along Lines", crate::checked!("aluminium.members", members))
        .at("ALUMINIUM", "CREATE").icon("frame_member").enabled(super::not_in_sketch)
        .contract(&MEMBERS_CONTRACT).output(crate::schema::output_schema_for::<FeatureResult>)
        .params("sketch: id|name (2D or 3D); lines?: straight curve ids (default all present and future); model?: 2020|3030|4040; anchor?: origin|center|centroid|top_left|top|top_right|left|right|bottom_left|bottom|bottom_right; angle?: angle expression; name?, body_name?, names?. Creates one fixed-section member per line, associated with the sketch, as one undo step. Select line groups separately to mix models. Does not trim ends at junctions"),
    CommandSpec::new("aluminium.profile", "Create Aluminium Profile", crate::checked!("aluminium.profile", create))
        .at("ALUMINIUM", "CREATE").icon("frame_member").enabled(super::not_in_sketch)
        .contract(&CONTRACT).output(crate::schema::output_schema_for::<FeatureResult>)
        .params("attachment?: {body: stock member, port: start|end|x_positive|x_negative|y_positive|y_negative, position?: 0–1 expression (default 0.5), gap?: length}; model?: 2020|3030|4040 (default 2020); length: length expression; origin?: [x,y,z]; axis?: X|Y|Z (default Z); angle?: angle expression; name?, body_name?. Creates one editable true solid with four open T-slots, diagonal webs and a through bore. Section dimensions are fixed by model and cannot be edited"),
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
    let attachment = crate::aluminium_ports::attachment(s, p.get("attachment"))?;
    let result = super::features::add_feature(
        s,
        &params,
        FeatureKind::AluminiumProfile { model, length, origin, axis: Dir(axis), reference: Dir(reference), angle, attachment },
    )?;
    if let Some(id) = result.get("feature").and_then(Value::as_u64)
        && let Some(name) = s.world_state().bodies.iter().find(|b| b.feature == id).map(|b| b.name.clone())
    {
        s.selection = vec![crate::Sel::Body { name }];
    }
    Ok(result)
}

fn connector(s: &mut Session, p: &Value) -> Result<Value> {
    let params = crate::stock_connectors::import_params(s, p)?;
    super::file::insert_step(s, &params)
}

fn members(s: &mut Session, p: &Value) -> Result<Value> {
    for key in ["profile", "profile_sketch", "stock_profile", "size", "width", "height", "slot_width", "bore_diameter", "wall", "scale"] {
        if p.get(key).is_some() {
            return Err(bad("aluminium.members", "choose a fixed catalogue model; custom sections and scaling are not supported"));
        }
    }
    let mut params = p.clone();
    let model = match p.get("model") {
        None => "2020",
        Some(v) => v.as_str().ok_or_else(|| bad("aluminium.members", "model must be a catalogue id"))?,
    };
    params["stock_profile"] = json!(model);
    if let Some(obj) = params.as_object_mut() {
        obj.remove("model");
    }
    super::frame::member(s, &params)
}

fn skeleton(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "aluminium.frame";
    let width = req_expr(cmd, p, "width")?;
    let height = req_expr(cmd, p, "height")?;
    let mut values = Vec::new();
    for e in [&width, &height] {
        let value = s.doc.eval(&s.doc.with_design_unit(e, Kind::Length), Kind::Length).map_err(|e| bad(cmd, e.to_string()))?;
        if !(0.1..=100_000.0).contains(&value) {
            return Err(bad(cmd, "skeleton dimensions must be 0.1–100000 mm"));
        }
        values.push(value);
    }
    let plane = match p.get("plane") {
        None => "XY",
        Some(v) => v.as_str().ok_or_else(|| bad(cmd, "plane must be XY, XZ or YZ"))?,
    };
    if !["XY", "XZ", "YZ"].contains(&plane) {
        return Err(bad(cmd, "plane must be XY, XZ or YZ"));
    }
    let mut trial = s.scratch();
    let sketch = trial.execute("sketch.create", &json!({"plane":plane,"name":p.get("name").and_then(Value::as_str).unwrap_or("Frame skeleton")}))?;
    // Numeric corners are only seeds in mm; the actual width and height remain driving expressions.
    let drawn = trial.execute(
        "sketch.draw.dimensioned",
        &json!({
            "command":"sketch.rectangle.two_point", "params":{"p0":[0,0],"p1":values},
            "dimensions":{"width":width,"height":height}, "locate":true
        }),
    )?;
    trial.execute("sketch.finish", &json!({}))?;
    let id = sketch["sketch"].as_u64().ok_or_else(|| bad(cmd, "sketch creation returned no id"))?;
    let lines: Vec<String> = drawn["curves"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    trial.selection = lines.iter().map(|line| crate::Sel::ExternalSketchCurve { sketch: id, id: line.clone() }).collect();
    trial.undo.clear();
    s.accept_computed(trial);
    Ok(json!({"sketch":id,"lines":lines,"dimensions":drawn["dimensions"]}))
}
