//! Read the existing feature graph without evaluating geometry or changing history.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::CommandSpec;
use crate::schema::view_navigation::{NodeGraph, NodeGraphLink, NodeGraphNode, NodeGraphRequest};
use crate::{Result, Session};

const CONTRACT: crate::schema::TypedContract<NodeGraphRequest> = crate::schema::TypedContract::new();
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("view.node_graph", "Node View", crate::checked!("view.node_graph", graph))
        .icon("graph").noundo().doc_only()
        .contract(&CONTRACT).output(crate::schema::output_schema_for::<NodeGraph>)
        .params("open?: bool (default true) → open, nodes, links; read-only graph of sketch, body, feature and named feature-parameter dependencies. Uses retained model states, never evaluates geometry. The UI applies open; headless calls never open a window"),
];

fn graph(s: &mut Session, p: &Value) -> Result<Value> {
    let request: NodeGraphRequest = serde_json::from_value(p.clone()).map_err(|e| crate::params::bad("view.node_graph", e.to_string()))?;
    let mut nodes = Vec::new();
    let mut links = BTreeSet::new();
    let mut parameters = BTreeMap::new();
    let mut prior = BTreeSet::new();
    let mut producers = BTreeMap::new();
    for (index, feature) in s.doc.features.iter().enumerate() {
        let mut inputs: BTreeSet<u64> = feature.kind.input_sketches().into_iter().collect();
        for before in &s.doc.features[..index] {
            if feature.kind.names_feature(before.id, &before.name) {
                inputs.insert(before.id);
            }
        }
        let before = s.model.state_before(feature.id);
        for name in feature.kind.input_bodies().iter().chain(&feature.participants) {
            if let Some(body) = before.body(name) {
                inputs.insert(producers.get(name).copied().unwrap_or(body.feature));
            }
        }
        // Geometry identity is shared across retained snapshots. Track the last step
        // that changed a body, rather than ModelBody.feature (its original creator).
        // This includes implicit joins/cuts and edge operations with inferred targets.
        if let Some(result) = s.model.result(feature.id) {
            for body in &result.state.bodies {
                if let Some(previous) = before.body(&body.name) {
                    if !previous.body.same(&body.body) || previous.source_transform != body.source_transform || previous.component != body.component {
                        inputs.insert(producers.get(&body.name).copied().unwrap_or(previous.feature));
                        producers.insert(body.name.clone(), feature.id);
                    }
                } else {
                    producers.insert(body.name.clone(), feature.id);
                }
            }
            for body in &before.bodies {
                if result.state.body(&body.name).is_none() {
                    inputs.insert(producers.get(&body.name).copied().unwrap_or(body.feature));
                    producers.remove(&body.name);
                }
            }
        }
        let expressions = feature.kind.inputs();
        for (_, expression, _) in &expressions {
            for name in solidcraft_doc::expr::references(expression) {
                if let Some(id) = parameters.get(&name) {
                    inputs.insert(*id);
                }
            }
        }
        for name in &feature.param_names {
            parameters.insert(name.clone(), feature.id);
        }
        for source in inputs.into_iter().filter(|id| prior.contains(id)) {
            links.insert((source, feature.id));
        }
        let result = s.model.result(feature.id);
        nodes.push(NodeGraphNode {
            id: feature.id,
            name: feature.name.clone(),
            kind: feature.kind.kind_key().into(),
            component: feature.component,
            suppressed: feature.suppressed,
            rolled_back: s.doc.marker.is_some_and(|marker| index >= marker),
            error: result.and_then(|result| result.error.clone()),
            warning: result.and_then(|result| result.warning.clone()),
            parameters: expressions.into_iter().map(|(label, value, _)| format!("{label}: {value}")).collect(),
        });
        prior.insert(feature.id);
    }
    let graph = NodeGraph { open: request.open, nodes, links: links.into_iter().map(|(source, target)| NodeGraphLink { source, target }).collect() };
    serde_json::to_value(graph).map_err(|e| crate::EngineError::Other(e.to_string()))
}
