//! Resolve stock-member interfaces shared by commands and direct viewport interaction.
use crate::{
    Result, Session,
    params::{bad, expr},
};
use serde_json::Value;
use solidcraft_doc::{
    aluminium::{Attachment, MemberFrame},
    expr::Kind,
};
use solidcraft_geom::Vec3;

#[derive(Clone, Debug)]
pub struct MemberInfo {
    pub frame: MemberFrame,
}

pub fn member(session: &Session, name: &str) -> Result<MemberInfo> {
    let world = session.world_state();
    let body = world.body(name).ok_or_else(|| bad("aluminium.profile", "the selected member no longer exists"))?;
    let frame = solidcraft_doc::aluminium::member_frame(&session.doc, body)?;
    Ok(MemberInfo { frame })
}

pub fn attachment(session: &Session, value: Option<&Value>) -> Result<Option<Attachment>> {
    let Some(value) = value.filter(|v| !v.is_null()) else { return Ok(None) };
    let cmd = "aluminium.profile";
    let body = value.get("body").and_then(Value::as_str).ok_or_else(|| bad(cmd, "attachment needs a member body"))?;
    let requested_path: Option<Vec<u64>> =
        value.get("path").map(|v| serde_json::from_value(v.clone()).map_err(|_| bad(cmd, "invalid member occurrence path"))).transpose()?;
    let state = session.model.state();
    let frames = session.doc.body_frames_with_source(&state);
    let (world, source, _, path, _) = frames
        .iter()
        .find(|(world, source, _, path, _)| (world == body || source == body) && requested_path.as_ref().is_none_or(|p| p == path))
        .ok_or_else(|| bad(cmd, "attached member placement is unavailable"))?;
    member(session, world)?;
    let port = serde_json::from_value(value.get("port").cloned().unwrap_or(Value::Null)).map_err(|_| bad(cmd, "invalid member interface"))?;
    let expression = |key, default: &str| -> Result<solidcraft_doc::Expr> {
        if value.get(key).is_none() {
            return Ok(default.into());
        }
        expr(value, key).ok_or_else(|| bad(cmd, format!("attachment {key} must be an expression")))
    };
    let position = expression("position", "0.5")?;
    let gap = expression("gap", "0 mm")?;
    let fraction = session.doc.eval(&position, Kind::Unitless)?;
    if !(0.0..=1.0).contains(&fraction) {
        return Err(bad(cmd, "position must be between 0 and 1"));
    }
    let distance = session.doc.eval(&session.doc.with_design_unit(&gap, Kind::Length), Kind::Length)?;
    if !distance.is_finite() || distance.abs() > 100_000.0 {
        return Err(bad(cmd, "invalid attachment gap"));
    }
    let author_path = match value.get("author_path") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| bad(cmd, "invalid author occurrence path"))?,
        None => session.active_occurrence_path.clone().unwrap_or_else(|| session.doc.first_path(session.active_component)),
    };
    if path.len() > 64 || author_path.len() > 64 {
        return Err(bad(cmd, "attachment occurrence path is too deep"));
    }
    Ok(Some(Attachment { body: source.clone().into(), port, position, gap, path: path.clone(), author_path }))
}

/// World-space base, growth axis and section reference for the placement handle.
pub fn placement(session: &Session, params: &Value) -> Result<(Vec3, Vec3, Vec3)> {
    if let Some(mut mount) = attachment(session, params.get("attachment"))? {
        mount.author_path.clear();
        let frame = solidcraft_doc::aluminium::attachment_frame(&session.doc, &session.model.state(), 0, &mount)?;
        let position = session.doc.eval(&mount.position, Kind::Unitless)?;
        let gap = session.doc.eval(&session.doc.with_design_unit(&mount.gap, Kind::Length), Kind::Length)?;
        return Ok(frame.port(mount.port, position, gap));
    }
    let origin =
        crate::cmd::features::point_param(session, params, "origin", "aluminium.profile")?.unwrap_or_else(|| solidcraft_doc::point_expr(Vec3::ZERO));
    let values = origin.iter().map(|e| session.doc.eval(e, Kind::Length)).collect::<solidcraft_doc::Result<Vec<_>>>()?;
    let axis = match params.get("axis").and_then(Value::as_str) {
        Some("X") => Vec3::X,
        Some("Y") => Vec3::Y,
        Some("Z") | None if params.get("axis").is_none_or(Value::is_string) => Vec3::Z,
        _ => params.get("axis").and_then(crate::params::vec3).and_then(|a| a.normalized()).ok_or_else(|| bad("aluminium.profile", "invalid axis"))?,
    };
    let reference = solidcraft_doc::frame::member_axes(axis, 0.0).map(|a| a.0).ok_or_else(|| bad("aluminium.profile", "invalid section frame"))?;
    Ok((Vec3::new(values[0], values[1], values[2]), axis, reference))
}
