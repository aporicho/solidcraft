//! Fixed manufacturer CAD catalogue. CAD bytes are installed in a separate local data cache.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Result, Session, params::bad};

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Resource {
    pub url: String,
    pub kind: String,
}

/// Audit of a real mating interface, distinct from the ability to import the CAD file.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Installation {
    pub state: String,
    pub role: String,
    pub target: String,
    pub reason: String,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub section: Option<[f64; 2]>,
    #[serde(default)]
    pub seat: String,
    #[serde(default)]
    pub excluded_stock: Vec<String>,
    /// Profiles with a completed native interference check for this installation frame.
    #[serde(default)]
    pub cleared_stock: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
    #[serde(default)]
    pub thickness_mismatch: Option<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Connector {
    #[serde(default)]
    pub installation: Installation,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub compatible_slots: Vec<u32>,
    #[serde(default)]
    pub profile_type: String,
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub import_error: String,
    #[serde(default)]
    pub geometry: String,
    pub category: String,
    pub interface: String,
    pub placement: String,
    /// Supplier-CAD datum at the nominal member surface; normal points out of the member.
    pub anchor: [f64; 3],
    pub normal: [f64; 3],
    /// Unit tangent aligned with the parent length on sides, or the chosen section axis at ends.
    pub along: [f64; 3],
    pub id: String,
    pub series: u32,
    pub compatible_series: Vec<u32>,
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

impl Connector {
    pub fn has_model(&self) -> bool {
        !self.filename.is_empty() && !self.step_sha256.is_empty()
    }

    pub fn insertable(&self) -> bool {
        self.has_model() && self.availability != "import_failed"
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let text = format!(
            "{} {} {} {} {} {}",
            self.label,
            self.part_number,
            self.id,
            self.profile_type,
            self.compatible_series.iter().map(u32::to_string).collect::<Vec<_>>().join(" "),
            self.compatible_slots.iter().map(u32::to_string).collect::<Vec<_>>().join(" ")
        )
        .to_lowercase();
        query.split_whitespace().all(|word| text.contains(&word.to_lowercase()))
    }

    pub fn compatible(&self, series: f64, interface: &str) -> bool {
        if !self.insertable() || self.installation.state != "datum_checked" || !self.installation.interfaces.iter().any(|i| i == interface) {
            return false;
        }
        let stock = format!("{0:.0}{0:.0}", series);
        if !self.installation.cleared_stock.contains(&stock) {
            return false;
        }
        if self.installation.excluded_stock.contains(&stock)
            || self.installation.section.is_some_and(|s| s.iter().any(|size| (size - series).abs() > 1e-6))
        {
            return false;
        }
        let slot = if series == 20.0 { 6 } else { 8 };
        let profile_type = if series == 40.0 { "I" } else { "B" };
        let known_series = self.compatible_series.iter().any(|s| f64::from(*s) == series);
        let known_slot = self.compatible_slots.contains(&slot);
        if (!self.compatible_series.is_empty() && !known_series)
            || (self.compatible_series.is_empty() && !known_slot)
            || (!self.compatible_slots.is_empty() && !known_slot)
            || (!self.profile_type.is_empty() && !self.profile_type.split('/').any(|t| t == profile_type))
        {
            return false;
        }
        true
    }
}

fn stored_catalogue() -> Result<&'static Vec<Connector>> {
    static MODELS: std::sync::OnceLock<std::result::Result<Vec<Connector>, String>> = std::sync::OnceLock::new();
    MODELS
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../enhancements/connector-catalog.json")).map_err(|e| format!("connector catalogue: {e}"))
        })
        .as_ref()
        .map_err(|e| crate::EngineError::Other(e.clone()))
}

pub fn catalogue() -> Result<Vec<Connector>> {
    Ok(stored_catalogue()?.clone())
}
pub fn model(id: &str) -> Result<Connector> {
    stored_catalogue()?.iter().find(|m| m.id == id).cloned().ok_or_else(|| bad("aluminium.connector", "unknown fixed connector model"))
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
    if !model.insertable() {
        return Err(bad(
            cmd,
            if model.has_model() {
                "the manufacturer model could not be read; see its source resources"
            } else {
                "this catalogue item has reference documents only; no supported 3D model is supplied"
            },
        ));
    }
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
            serde_json::from_value(mount.get("port").or_else(|| mount.get("end")).cloned().unwrap_or(Value::Null))
                .map_err(|_| bad(cmd, "choose a member end"))?;
        let side = mount.get("side").map_or(Some(0), Value::as_u64).filter(|i| *i < 4).ok_or_else(|| bad(cmd, "connector side must be 0–3"))?;
        let frame = crate::aluminium_ports::member(session, body)?.frame;
        let gap = crate::params::expr(mount, "gap").unwrap_or_else(|| "0 mm".into());
        let gap = session.doc.eval(&session.doc.with_design_unit(&gap, solidcraft_doc::expr::Kind::Length), solidcraft_doc::expr::Kind::Length)?;
        if !gap.is_finite() || gap.abs() > 100_000.0 {
            return Err(bad(cmd, "invalid connector offset"));
        }
        let kind = mount.get("interface").and_then(Value::as_str).unwrap_or("end");
        if model.installation.state != "datum_checked" {
            return Err(bad(cmd, "this part needs a separately defined mating interface; use free placement and its supplier drawing"));
        }
        if !model.compatible(frame.size, kind) || (port.is_end() != matches!(kind, "end" | "bore")) {
            return Err(bad(cmd, "connector is incompatible with this installation interface"));
        }
        let position = mount
            .get("position")
            .map_or(Some(0.5), Value::as_f64)
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or_else(|| bad(cmd, "interface position must be between 0 and 1"))?;
        let (mut at, normal, _) = frame.port(port, position, gap);
        if model.installation.seat == "under_slot_lip" {
            let lip = solidcraft_doc::aluminium::slot_lip_depth(&frame.model)
                .map_err(|_| bad(cmd, "the profile's slot lip dimensions are unavailable"))?;
            at = at - normal * lip;
        }
        let radial = match side {
            0 => frame.x,
            1 => frame.y,
            2 => -frame.x,
            _ => -frame.y,
        };
        if let Some(base) = crate::connector_ports::definitions(&model.id).first().filter(|_| port.is_end()) {
            use solidcraft_geom::Vec3;
            let vector = |p: [f64; 3]| Vec3::new(p[0], p[1], p[2]);
            let local_n = vector(base.normal);
            let local_u = vector(base.reference);
            let local_v = local_n.cross(local_u);
            let n = -normal;
            let u = radial;
            let v = n.cross(u);
            let transform = |p: Vec3| u * p.dot(local_u) + v * p.dot(local_v) + n * p.dot(local_n);
            let x = transform(Vec3::X);
            let y = transform(Vec3::Y);
            let z = transform(Vec3::Z);
            let origin = at - transform(vector(base.origin));
            output["matrix"] = json!([[x.x, y.x, z.x, origin.x], [x.y, y.y, z.y, origin.y], [x.z, y.z, z.z, origin.z], [0, 0, 0, 1]]);
        } else if model.placement == "angle" {
            let n = if port.is_end() {
                normal
            } else if position < 0.5 {
                -frame.axis
            } else {
                frame.axis
            };
            let radial = if port.is_end() { radial } else { normal };
            let origin = if port.is_end() { at + radial * (frame.size / 2.0) } else { at };
            let z = n.cross(radial);
            output["matrix"] = json!([[n.x, radial.x, z.x, origin.x], [n.y, radial.y, z.y, origin.y], [n.z, radial.z, z.z, origin.z], [0, 0, 0, 1]]);
        } else {
            use solidcraft_geom::Vec3;
            let vector = |v: [f64; 3]| Vec3::new(v[0], v[1], v[2]);
            let local_n = vector(model.normal);
            let local_u = vector(model.along);
            let local_v = local_n.cross(local_u);
            let u = if port.is_end() { radial } else { frame.axis };
            let v = normal.cross(u);
            let transform = |p: Vec3| u * p.dot(local_u) + v * p.dot(local_v) + normal * p.dot(local_n);
            let x = transform(Vec3::new(1.0, 0.0, 0.0));
            let y = transform(Vec3::new(0.0, 1.0, 0.0));
            let z = transform(Vec3::new(0.0, 0.0, 1.0));
            let origin = at - transform(vector(model.anchor));
            output["matrix"] = json!([[x.x, y.x, z.x, origin.x], [x.y, y.y, z.y, origin.y], [x.z, y.z, z.z, origin.z], [0, 0, 0, 1]]);
        }
        // Local adjustment after prepositioning; never changes the fixed manufacturer's geometry.
        let mut placed = solidcraft_doc::IDENTITY;
        for (col, values) in placed.iter_mut().enumerate() {
            for (row, value) in values.iter_mut().enumerate() {
                *value = output["matrix"][row][col].as_f64().ok_or_else(|| bad(cmd, "invalid connector placement"))?;
            }
        }
        let adjusted = solidcraft_doc::mat_mul(&placed, &matrix);
        output["matrix"] = json!((0..4).map(|row| (0..4).map(|col| adjusted[col][row]).collect::<Vec<_>>()).collect::<Vec<_>>());
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
        component.properties.insert("profile_type".into(), model.profile_type.clone());
        component.properties.insert("profile_series".into(), model.compatible_series.iter().map(u32::to_string).collect::<Vec<_>>().join("/"));
        component.properties.insert("slot".into(), model.compatible_slots.iter().map(u32::to_string).collect::<Vec<_>>().join("/"));
    }
    output["part_number"] = json!(model.part_number);
    output["supplier"] = json!(model.supplier);
    output["source_url"] = json!(model.product_url);
    Ok(())
}
