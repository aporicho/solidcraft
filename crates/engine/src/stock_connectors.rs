//! Fixed manufacturer CAD catalogue. CAD bytes are installed in a separate local data cache.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Result, Session, params::bad};

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Connector {
    pub id: String,
    pub series: u32,
    pub slot: u32,
    pub supplier: String,
    pub part_number: String,
    pub label: String,
    pub product_url: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub step_member: String,
    pub filename: String,
    pub step_sha256: String,
}

pub fn catalogue() -> Result<Vec<Connector>> {
    serde_json::from_str(include_str!("../../../enhancements/connector-catalog.json"))
        .map_err(|error| crate::EngineError::Other(format!("connector catalogue: {error}")))
}

pub fn cache_dir() -> PathBuf {
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("/connectors")
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var_os("SOLIDCRAFT_CONNECTOR_LIBRARY").map(PathBuf::from).unwrap_or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
                .unwrap_or_else(std::env::temp_dir)
                .join("solidcraft/connectors")
        })
    }
}

/// Resolve stock selection and rigid placement to the ordinary STEP import command.
/// Both the interactive tool and headless command use this path; no resizing is offered.
pub fn import_params(session: &Session, params: &Value) -> Result<Value> {
    let cmd = "aluminium.connector";
    for key in ["scale", "size", "length", "width", "height"] {
        if params.get(key).is_some() {
            return Err(bad(cmd, "connector geometry is fixed by model; only rigid placement is supported"));
        }
    }
    let id = params.get("model").and_then(Value::as_str).unwrap_or("motedis-s6bbr20");
    let model = catalogue()?.into_iter().find(|entry| entry.id == id).ok_or_else(|| bad(cmd, "unknown fixed connector model"))?;
    let path = cache_dir().join(&model.filename);
    let path = path.to_str().ok_or_else(|| bad(cmd, "invalid connector cache path"))?;
    if !solidcraft_io::vfs::exists(path) {
        return Err(bad(cmd, "the selected connector CAD model is not installed"));
    }
    let origin =
        crate::cmd::features::point_param(session, params, "origin", cmd)?.unwrap_or_else(|| solidcraft_doc::point_expr(solidcraft_geom::Vec3::ZERO));
    let rotation = match params.get("rotation") {
        None => [0.0; 3],
        Some(Value::Array(values)) if values.len() == 3 => {
            let angle = |index| -> Result<f64> {
                let expression = crate::params::req_expr(cmd, &json!({"angle":values[index]}), "angle")?;
                if let Ok(degrees) = expression.parse::<f64>() {
                    if !degrees.is_finite() {
                        return Err(bad(cmd, "rotation must be finite"));
                    }
                    Ok(degrees.to_radians())
                } else {
                    session.doc.eval(&expression, solidcraft_doc::expr::Kind::Angle).map_err(|error| bad(cmd, error.to_string()))
                }
            };
            [angle(0)?, angle(1)?, angle(2)?]
        }
        _ => return Err(bad(cmd, "rotation needs three angles")),
    };
    let (sx, cx) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sz, cz) = rotation[2].sin_cos();
    let rx = [[1., 0., 0., 0.], [0., cx, sx, 0.], [0., -sx, cx, 0.], [0., 0., 0., 1.]];
    let ry = [[cy, 0., -sy, 0.], [0., 1., 0., 0.], [sy, 0., cy, 0.], [0., 0., 0., 1.]];
    let rz = [[cz, sz, 0., 0.], [-sz, cz, 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];
    let mut matrix = solidcraft_doc::mat_mul(&rz, &solidcraft_doc::mat_mul(&ry, &rx));
    for (index, expression) in origin.iter().enumerate() {
        matrix[3][index] = session.doc.eval(expression, solidcraft_doc::expr::Kind::Length).map_err(|error| bad(cmd, error.to_string()))?;
    }
    let rows: Vec<Vec<f64>> = (0..4).map(|row| (0..4).map(|col| matrix[col][row]).collect()).collect();
    let mut output = json!({"path":path,"name":params.get("name").and_then(Value::as_str).unwrap_or(&model.label),
        "matrix":rows,"stock_connector":model.id});
    if let Some(mount) = params.get("mount") {
        let body = mount.get("body").and_then(Value::as_str).ok_or_else(|| bad(cmd, "select a stock member for the connector"))?;
        let port: solidcraft_doc::aluminium::Port =
            serde_json::from_value(mount.get("end").cloned().unwrap_or(Value::Null)).map_err(|_| bad(cmd, "choose a member end"))?;
        if !port.is_end() {
            return Err(bad(cmd, "connectors are placed from a member end"));
        }
        let side = mount.get("side").map_or(Some(0), Value::as_u64).filter(|i| *i < 4).ok_or_else(|| bad(cmd, "connector side must be 0–3"))?;
        let frame = crate::aluminium_ports::member(session, body)?.frame;
        if f64::from(model.series) != frame.size {
            return Err(bad(cmd, "connector series does not match the selected member"));
        }
        let gap = crate::params::expr(mount, "gap").unwrap_or_else(|| "0 mm".into());
        let gap = session.doc.eval(&session.doc.with_design_unit(&gap, solidcraft_doc::expr::Kind::Length), solidcraft_doc::expr::Kind::Length)?;
        if !gap.is_finite() || gap.abs() > 100_000.0 {
            return Err(bad(cmd, "invalid connector offset"));
        }
        let (end, n, _) = frame.port(port, 0.5, gap);
        let radial = match side {
            0 => frame.x,
            1 => frame.y,
            2 => -frame.x,
            _ => -frame.y,
        };
        let origin = end + radial * (frame.size / 2.0);
        // These fixed Motedis models use the two outer mounting planes X=0 and Y=0;
        // their alignment tabs project into +X / -Y and their width is centred on Z=0.
        // -X extends back along the parent, +Y extends out from the selected outer side.
        let z = n.cross(radial);
        output["matrix"] = json!([[n.x, radial.x, z.x, origin.x], [n.y, radial.y, z.y, origin.y], [n.z, radial.z, z.z, origin.z], [0, 0, 0, 1]]);
    }
    Ok(output)
}

/// Preserve catalogue identity in the part list without embedding vendor CAD in this repo.
pub fn annotate(session: &mut Session, output: &mut Value, id: &str) -> Result<()> {
    let model = catalogue()?.into_iter().find(|entry| entry.id == id).ok_or_else(|| bad("aluminium.connector", "unknown connector model"))?;
    let component_id = output["component"].as_u64().ok_or_else(|| bad("aluminium.connector", "import did not create a component"))?;
    if let Some(component) = session.doc_mut().components.iter_mut().find(|component| component.id == component_id) {
        component.metadata.set(solidcraft_doc::metadata::Owner::Component, "part_number", json!(model.part_number))?;
        component.properties.insert("supplier".into(), model.supplier.clone());
        component.properties.insert("stock_connector".into(), model.id.clone());
        component.properties.insert("source_url".into(), model.product_url.clone());
        component.properties.insert("profile_series".into(), model.series.to_string());
        component.properties.insert("slot".into(), model.slot.to_string());
    }
    output["part_number"] = json!(model.part_number);
    output["supplier"] = json!(model.supplier);
    output["source_url"] = json!(model.product_url);
    Ok(())
}
