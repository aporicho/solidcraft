//! Named CAD installation ports. Missing neighbours leave ports open; they never block insertion.
use crate::{params::bad, Result, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solidcraft_doc::{aluminium::ConnectorDatum, FeatureKind};
use solidcraft_geom::Vec3;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Port {
    pub id: String,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub reference: [f64; 3],
    pub size: f64,
    pub slot: u32,
}

/// Verified cube mounting planes, independent of triangulation and face numbering.
pub fn definitions(id: &str) -> Vec<Port> {
    // Motedis CC3D20x20M4x5 has three slot tongues on different datum planes.
    // These frames follow the pinned STEP body (CAD hash in connector-catalog.json)
    // and supplier drawing; they are not a symmetric 20 mm cube's outside faces.
    if id == "motedis-cc3d20x20m4x5-6c2938c7" {
        return [
            ("x_positive", [10.0, 10.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ("y_positive", [0.0, 16.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
            ("z_positive", [0.0, 10.0, 10.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ]
        .into_iter()
        .map(|(id, origin, normal, reference)| Port { id: id.into(), origin, normal, reference, size: 20.0, slot: 6 })
        .collect();
    }
    let (size, slot, axes): (f64, u32, &[&str]) = match id {
        "motedis-s8bcc3d30" => (30.0, 8, &["z_negative", "x_positive", "y_positive"]),
        "motedis-s8bcc2d30" => (30.0, 8, &["z_negative", "y_positive"]),
        "motedis-s8icu403d" => (40.0, 8, &["z_negative", "x_positive", "y_positive"]),
        "motedis-cube-connector-40-2d-i-type-slot-8" => (40.0, 8, &["z_negative", "x_negative"]),
        "motedis-cube-connector-20-3d-i-type-slot-5-a861d1fe" => (20.0, 5, &["z_negative", "x_positive", "y_positive"]),
        "motedis-cube-connector-20-2d-i-type-slot-5-c34e1ea2" => (20.0, 5, &["z_negative", "x_positive"]),
        "motedis-cube-connector-45-3d-b-type-slot-10-5a3c2123" => (45.0, 10, &["z_negative", "x_positive", "y_positive"]),
        "motedis-cube-connector-45-2d-b-type-slot-10-e4320ec0" => (45.0, 10, &["z_negative", "y_positive"]),
        _ => return Vec::new(),
    };
    axes.iter()
        .map(|id| {
            let normal = match *id {
                "x_positive" => Vec3::X,
                "x_negative" => -Vec3::X,
                "y_positive" => Vec3::Y,
                _ => -Vec3::Z,
            };
            let reference = if normal.z.abs() > 0.5 { Vec3::X } else { Vec3::Z };
            let origin = normal * (size / 2.0);
            Port {
                id: (*id).into(),
                origin: [origin.x, origin.y, origin.z],
                normal: [normal.x, normal.y, normal.z],
                reference: [reference.x, reference.y, reference.z],
                size,
                slot,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct WorldPort {
    pub id: String,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub reference: [f64; 3],
    pub size: f64,
    pub slot: u32,
    pub occupied: Option<String>,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct Inspection {
    pub body: String,
    pub model: String,
    pub ports: Vec<WorldPort>,
}

pub struct Instance {
    pub model: String,
    pub body: String,
    pub source: String,
    pub path: Vec<u64>,
    pub root_matrix: solidcraft_doc::Mat,
    pub body_matrix: solidcraft_doc::Mat,
    pub cad_sha256: String,
}

pub fn instance(session: &Session, name: &str) -> Result<Instance> {
    let state = session.model.state();
    let (_, source, _, path, body_matrix) = session
        .doc
        .body_frames_with_source(&state)
        .into_iter()
        .find(|(world, ..)| world == name)
        .ok_or_else(|| bad("aluminium.connector.ports", "the selected part no longer exists"))?;
    let candidate = session
        .doc
        .placements()
        .into_iter()
        .filter(|(_, p, _)| path.starts_with(p))
        .filter_map(|(id, p, m)| {
            let component = session.doc.components.iter().find(|c| c.id == id)?;
            Some((p.len(), component.properties.get("stock_connector")?.clone(), m))
        })
        .max_by_key(|(depth, ..)| *depth)
        .ok_or_else(|| bad("aluminium.connector.ports", "select a library connector"))?;
    let model = crate::stock_connectors::model(&candidate.1)?;
    Ok(Instance { model: model.id, body: name.into(), source, path, root_matrix: candidate.2, body_matrix, cad_sha256: model.step_sha256 })
}

fn vector(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}
fn array(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

impl Instance {
    pub fn datum(&self, port: &str) -> Result<ConnectorDatum> {
        let p = definitions(&self.model).into_iter().find(|p| p.id == port).ok_or_else(|| bad("aluminium.profile", "unknown connector port"))?;
        let inverse = solidcraft_doc::mat_inverse(&self.body_matrix).ok_or_else(|| bad("aluminium.profile", "invalid connector placement"))?;
        let m = solidcraft_doc::mat_mul(&inverse, &self.root_matrix);
        Ok(ConnectorDatum {
            model: self.model.clone(),
            port: p.id,
            cad_sha256: self.cad_sha256.clone(),
            origin: array(solidcraft_doc::apply_point(&m, vector(p.origin))),
            normal: array(solidcraft_doc::apply_vector(&m, vector(p.normal))),
            reference: array(solidcraft_doc::apply_vector(&m, vector(p.reference))),
            size: p.size,
            slot: p.slot,
        })
    }
    pub fn ports(&self, session: &Session) -> Vec<WorldPort> {
        let world = session.world_state();
        let members: Vec<_> =
            world.bodies.iter().filter_map(|b| solidcraft_doc::aluminium::member_frame(&session.doc, b).ok().map(|f| (b.name.clone(), f))).collect();
        definitions(&self.model)
            .into_iter()
            .map(|p| {
                let origin = solidcraft_doc::apply_point(&self.root_matrix, vector(p.origin));
                let normal = solidcraft_doc::apply_vector(&self.root_matrix, vector(p.normal));
                let reference = solidcraft_doc::apply_vector(&self.root_matrix, vector(p.reference));
                let linked = session.doc.features.iter().filter(|f| !f.suppressed).find_map(|f| match &f.kind {
                    FeatureKind::AluminiumProfile { attachment: Some(a), .. }
                        if a.body.as_ref() == self.source && a.path == self.path && a.datum.as_ref().is_some_and(|d| d.port == p.id) =>
                    {
                        world.bodies.iter().find(|b| b.feature == f.id).map(|b| b.name.clone())
                    }
                    _ => None,
                });
                let occupied = linked.or_else(|| {
                    members.iter().find_map(|(name, frame)| {
                        if (frame.size - p.size).abs() > 1e-6 {
                            return None;
                        }
                        [(frame.origin, -frame.axis), (frame.origin + frame.axis * frame.length, frame.axis)]
                            .iter()
                            .any(|(at, n)| (*at - origin).len() < 1e-4 && n.dot(normal) < -0.999)
                            .then(|| name.clone())
                    })
                });
                WorldPort {
                    id: p.id,
                    origin: array(origin),
                    normal: array(normal),
                    reference: array(reference),
                    size: p.size,
                    slot: p.slot,
                    occupied,
                }
            })
            .collect()
    }
}

pub fn inspect(session: &mut Session, params: &Value) -> Result<Value> {
    let body = params.get("body").and_then(Value::as_str).ok_or_else(|| bad("aluminium.connector.ports", "body is required"))?;
    let instance = instance(session, body)?;
    Ok(json!(Inspection { body: body.into(), model: instance.model.clone(), ports: instance.ports(session) }))
}
