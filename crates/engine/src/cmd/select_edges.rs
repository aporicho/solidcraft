//! Fork selection enhancements: closed loops and planar exterior boundaries.
use std::collections::BTreeSet;

use serde_json::Value;
use solidcraft_geom::{Mesh, Plane};

use super::CommandSpec;
use crate::schema::edit::{EdgeSelectionParams, EdgeSelectionResult};
use crate::{EngineError, Result, Sel, Session};

const CONTRACT: crate::schema::TypedContract<EdgeSelectionParams> = crate::schema::TypedContract::new();
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("select.loop", "Select Loop", crate::checked!("select.loop", select_loop))
        .icon("select_loop").noundo().enabled(super::not_in_sketch)
        .contract(&CONTRACT).output(crate::schema::output_schema_for::<EdgeSelectionResult>)
        .params("body?, face?, edge?, add?: bool; expand picked edges to their closed loop on a planar face. A picked face restricts ambiguous edges; otherwise prefer the largest adjacent planar face. A face alone selects its outside loop. No geometry or history changes"),
    CommandSpec::new("select.outer_edges", "Select Outer Edges", crate::checked!("select.outer_edges", select_outer))
        .icon("select_outer_edges").noundo().enabled(super::not_in_sketch)
        .contract(&CONTRACT).output(crate::schema::output_schema_for::<EdgeSelectionResult>)
        .params("body?, face?, edge?, add?: bool; select only the outer boundary of picked planar faces, excluding hole and slot loops. An edge infers its largest adjacent planar face. No geometry or history changes"),
];

fn select_loop(s: &mut Session, p: &Value) -> Result<Value> {
    expand(s, p, false)
}
fn select_outer(s: &mut Session, p: &Value) -> Result<Value> {
    expand(s, p, true)
}

/// Recover the plane in world coordinates from the same mesh used for picking. Curved
/// faces are refused rather than flattened into a misleading outside loop.
fn face_plane(mesh: &Mesh, face: usize) -> Option<(Plane, f64)> {
    let mut plane = None;
    let mut area = 0.0;
    for (triangle, &id) in mesh.triangles.iter().zip(&mesh.tri_face) {
        if id as usize != face {
            continue;
        }
        let [ia, ib, ic] = *triangle;
        let (a, b, c) = (*mesh.positions.get(ia as usize)?, *mesh.positions.get(ib as usize)?, *mesh.positions.get(ic as usize)?);
        area += (b - a).cross(c - a).len() * 0.5;
        if plane.is_none() {
            plane = Plane::new(a, b - a, c - a);
        }
    }
    let plane = plane?;
    let tolerance = (mesh.bounds().diagonal() * 1e-7).max(1e-8);
    if !area.is_finite() || !tolerance.is_finite() || area <= 0.0 {
        return None;
    }
    mesh.face_edges(u32::try_from(face).ok()?)
        .iter()
        .all(|&edge| mesh.edges.get(edge).is_some_and(|line| line.iter().all(|p| p.is_finite() && plane.height(*p).abs() <= tolerance)))
        .then_some((plane, area))
}

fn expand(s: &mut Session, p: &Value, outer: bool) -> Result<Value> {
    let id = if outer { "select.outer_edges" } else { "select.loop" };
    let error = |message: &str| crate::params::bad(id, message);
    let request: EdgeSelectionParams = serde_json::from_value(p.clone()).map_err(|e| error(&e.to_string()))?;
    let state = s.world_state();
    let mut seeds = s.selection.clone();
    if let Some(body) = &request.body {
        seeds.clear();
        if let Some(index) = request.face {
            seeds.push(Sel::Face { body: body.clone(), index, point: Default::default() });
        }
        if let Some(index) = request.edge {
            seeds.push(Sel::Edge { body: body.clone(), index, point: Default::default() });
        }
    } else if request.face.is_some() || request.edge.is_some() {
        return Err(error("an explicit face or edge requires body"));
    }
    let selected_faces: Vec<_> =
        seeds.iter().filter_map(|s| if let Sel::Face { body, index, .. } = s { Some((body.as_str(), *index)) } else { None }).collect();
    let mut chosen: BTreeSet<(String, usize)> = BTreeSet::new();
    for seed in &seeds {
        let (body, seed_face, seed_edge) = match seed {
            Sel::Face { body, index, .. } => {
                if !outer && seeds.iter().any(|s| matches!(s, Sel::Edge {body: b,..} if b==body)) {
                    continue;
                }
                (body, Some(*index), None)
            }
            Sel::Edge { body, index, .. } => {
                if outer && selected_faces.iter().any(|(b, _)| *b == body) {
                    continue;
                }
                (body, None, Some(*index))
            }
            _ => continue,
        };
        let model = state.body(body).ok_or_else(|| error("selected body no longer exists"))?;
        let mesh = model.mesh();
        if mesh.display_only_edges {
            return Err(error("select a solid's real edges, not a triangle mesh"));
        }
        let mut faces = if let Some(face) = seed_face {
            vec![face]
        } else {
            let edge = seed_edge.ok_or_else(|| error("select a face or an edge first"))?;
            if !mesh.edge_pickable(edge) || mesh.edges.get(edge).is_none() {
                return Err(error("invalid selected edge"));
            }
            let restricted: Vec<_> = selected_faces.iter().filter(|(b, _)| *b == body).map(|(_, f)| *f).collect();
            if restricted.is_empty() { mesh.edge_faces.get(edge).into_iter().flatten().map(|f| *f as usize).collect() } else { restricted }
        };
        faces.sort_unstable();
        faces.dedup();
        let mut candidates: Vec<_> = faces.into_iter().filter_map(|face| face_plane(&mesh, face).map(|(plane, area)| (face, plane, area))).collect();
        candidates.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
        let mut picked = None;
        for (face, plane, _) in candidates {
            let face = u32::try_from(face).map_err(|_| error("invalid face index"))?;
            let loops = solidcraft_doc::project::face_boundary_loops(&mesh, face, &plane).map_err(|e| error(&e.to_string()))?;
            let found = if outer || seed_edge.is_none() {
                loops.first()
            } else {
                loops.iter().find(|items| seed_edge.is_some_and(|edge| items.contains(&edge)))
            };
            if let Some(items) = found {
                picked = Some(items.clone());
                break;
            }
        }
        let picked = picked.ok_or_else(|| error("select a closed boundary on a planar face; for an ambiguous edge select its face too"))?;
        for edge in picked {
            chosen.insert((body.clone(), edge));
        }
    }
    if chosen.is_empty() {
        return Err(error("select a planar face or one of its edges first"));
    }
    let edges = chosen.len();
    let mut result = if request.add { s.selection.clone() } else { Vec::new() };
    for (body, index) in chosen {
        let model = state.body(&body).ok_or_else(|| error("body no longer exists"))?;
        let mesh = model.mesh();
        let line = mesh.edges.get(index).ok_or_else(|| error("edge no longer exists"))?;
        let point = line.get(line.len() / 2).copied().ok_or_else(|| error("edge has no curve"))?;
        if !result.iter().any(|s| matches!(s, Sel::Edge {body:b,index:i,..} if b==&body && *i==index)) {
            result.push(Sel::Edge { body, index, point });
        }
    }
    s.selection = result;
    s.refresh_display();
    serde_json::to_value(EdgeSelectionResult { selected: s.selection.len(), edges }).map_err(|e| EngineError::Other(e.to_string()))
}
