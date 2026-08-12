use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as JsonValue;

use super::{AutomationEdge, AutomationNode};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AutomationGraphError {
    #[error("automation node id must be unique: {node_id}")]
    DuplicateNodeId { node_id: String },
    #[error("a non-empty automation graph requires one trigger node")]
    MissingTrigger,
    #[error("an automation graph supports exactly one trigger node")]
    MultipleTriggers,
    #[error("automation edge {edge_id} references missing {endpoint} node {node_id}")]
    DanglingEdge {
        edge_id: String,
        endpoint: &'static str,
        node_id: String,
    },
    #[error("automation graph contains a cycle")]
    Cycle,
}

pub fn validate_automation_graph(
    nodes: &[AutomationNode],
    edges: &[AutomationEdge],
) -> Result<(), AutomationGraphError> {
    let mut ids = BTreeSet::new();
    for node in nodes {
        if !ids.insert(node.id.clone()) {
            return Err(AutomationGraphError::DuplicateNodeId {
                node_id: node.id.clone(),
            });
        }
    }

    match nodes.iter().filter(|node| node.kind == "trigger").count() {
        0 if !nodes.is_empty() => return Err(AutomationGraphError::MissingTrigger),
        0 | 1 => {}
        _ => return Err(AutomationGraphError::MultipleTriggers),
    }

    for edge in edges {
        if !ids.contains(&edge.source) {
            return Err(AutomationGraphError::DanglingEdge {
                edge_id: edge.id.clone(),
                endpoint: "source",
                node_id: edge.source.clone(),
            });
        }
        if !ids.contains(&edge.target) {
            return Err(AutomationGraphError::DanglingEdge {
                edge_id: edge.id.clone(),
                endpoint: "target",
                node_id: edge.target.clone(),
            });
        }
    }

    automation_topological_order(nodes, edges).map(|_| ())
}

pub fn automation_topological_order(
    nodes: &[AutomationNode],
    edges: &[AutomationEdge],
) -> Result<Vec<String>, AutomationGraphError> {
    let ids = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let mut incoming = ids
        .iter()
        .map(|id| (id.clone(), 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeMap::<String, Vec<String>>::new();

    for edge in edges {
        if !ids.contains(&edge.source) {
            return Err(AutomationGraphError::DanglingEdge {
                edge_id: edge.id.clone(),
                endpoint: "source",
                node_id: edge.source.clone(),
            });
        }
        let Some(target_count) = incoming.get_mut(&edge.target) else {
            return Err(AutomationGraphError::DanglingEdge {
                edge_id: edge.id.clone(),
                endpoint: "target",
                node_id: edge.target.clone(),
            });
        };
        *target_count += 1;
        outgoing
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
    }
    for targets in outgoing.values_mut() {
        targets.sort();
    }

    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(nodes.len());
    while let Some(id) = ready.pop_first() {
        ordered.push(id.clone());
        for target in outgoing.get(&id).into_iter().flatten() {
            let count = incoming
                .get_mut(target)
                .expect("edge endpoints were validated before traversal");
            *count -= 1;
            if *count == 0 {
                ready.insert(target.clone());
            }
        }
    }

    if ordered.len() == nodes.len() {
        Ok(ordered)
    } else {
        Err(AutomationGraphError::Cycle)
    }
}

pub fn automation_initial_active_nodes(
    nodes: &[AutomationNode],
    edges: &[AutomationEdge],
) -> BTreeSet<String> {
    let triggers = nodes
        .iter()
        .filter(|node| node.kind == "trigger")
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    if !triggers.is_empty() {
        return triggers;
    }

    let targets = edges
        .iter()
        .map(|edge| edge.target.clone())
        .collect::<BTreeSet<_>>();
    nodes
        .iter()
        .filter(|node| !targets.contains(&node.id))
        .map(|node| node.id.clone())
        .collect()
}

pub fn automation_active_outgoing_edges<'a>(
    edges: &'a [AutomationEdge],
    node_id: &str,
    output: &JsonValue,
) -> Vec<&'a AutomationEdge> {
    let outgoing = edges
        .iter()
        .filter(|edge| edge.source == node_id)
        .collect::<Vec<_>>();
    if outgoing.is_empty() {
        return Vec::new();
    }

    let selected_handles = automation_selected_output_handles(output);
    let has_exact_selected_edge = outgoing.iter().any(|edge| {
        edge.source_handle
            .as_deref()
            .map(normalize_handle)
            .filter(|handle| handle != "default")
            .is_some_and(|handle| selected_handles.contains(&handle))
    });

    outgoing
        .into_iter()
        .filter(|edge| {
            let handle = edge.source_handle.as_deref().map(normalize_handle);
            match handle.as_deref() {
                Some("default") => {
                    output.get("routeKind").and_then(JsonValue::as_str) == Some("switch")
                        && !has_exact_selected_edge
                }
                Some(handle) => selected_handles.contains(handle),
                None => {
                    selected_handles.is_empty()
                        || selected_handles.contains("success")
                        || selected_handles.contains("true")
                }
            }
        })
        .collect()
}

pub fn automation_selected_output_handles(output: &JsonValue) -> BTreeSet<String> {
    let mut handles = BTreeSet::new();
    if let Some(handle) = output.get("selectedHandle").and_then(JsonValue::as_str) {
        let handle = normalize_handle(handle);
        if !handle.is_empty() {
            handles.insert(handle);
        }
    }
    if let Some(items) = output.get("selectedHandles").and_then(JsonValue::as_array) {
        for item in items {
            if let Some(handle) = item.as_str() {
                let handle = normalize_handle(handle);
                if !handle.is_empty() {
                    handles.insert(handle);
                }
            }
        }
    }
    handles
}

fn normalize_handle(handle: &str) -> String {
    handle.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationNodePosition;

    fn node(id: &str, kind: &str) -> AutomationNode {
        AutomationNode {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            position: AutomationNodePosition { x: 0.0, y: 0.0 },
            config: serde_json::json!({}),
        }
    }

    fn edge(id: &str, source: &str, target: &str, source_handle: Option<&str>) -> AutomationEdge {
        AutomationEdge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            source_handle: source_handle.map(str::to_owned),
            target_handle: None,
        }
    }

    #[test]
    fn graph_rejects_cycles_and_dangling_edges() {
        let nodes = vec![node("trigger", "trigger"), node("tool", "tool")];
        assert_eq!(
            validate_automation_graph(
                &nodes,
                &[
                    edge("forward", "trigger", "tool", None),
                    edge("back", "tool", "trigger", None),
                ],
            ),
            Err(AutomationGraphError::Cycle)
        );
        assert!(matches!(
            validate_automation_graph(&nodes, &[edge("missing", "trigger", "absent", None)]),
            Err(AutomationGraphError::DanglingEdge {
                endpoint: "target",
                ..
            })
        ));
    }

    #[test]
    fn topological_order_is_deterministic_across_input_order() {
        let nodes = vec![
            node("trigger", "trigger"),
            node("beta", "tool"),
            node("alpha", "tool"),
            node("final", "tool"),
        ];
        let edges = vec![
            edge("e3", "beta", "final", None),
            edge("e2", "trigger", "beta", None),
            edge("e4", "alpha", "final", None),
            edge("e1", "trigger", "alpha", None),
        ];
        let mut reversed_nodes = nodes.clone();
        reversed_nodes.reverse();
        let mut reversed_edges = edges.clone();
        reversed_edges.reverse();

        let expected = vec!["trigger", "alpha", "beta", "final"];
        assert_eq!(
            automation_topological_order(&nodes, &edges).unwrap(),
            expected
        );
        assert_eq!(
            automation_topological_order(&reversed_nodes, &reversed_edges).unwrap(),
            expected
        );
    }

    #[test]
    fn branch_handles_use_exact_match_then_switch_default() {
        let edges = vec![
            edge("paid", "branch", "paid-node", Some(" paid ")),
            edge("fallback", "branch", "fallback-node", Some("default")),
            edge("success", "branch", "success-node", None),
        ];

        let exact = automation_active_outgoing_edges(
            &edges,
            "branch",
            &serde_json::json!({"routeKind": "switch", "selectedHandle": "PAID"}),
        );
        assert_eq!(
            exact
                .iter()
                .map(|edge| edge.id.as_str())
                .collect::<Vec<_>>(),
            vec!["paid"]
        );

        let fallback = automation_active_outgoing_edges(
            &edges,
            "branch",
            &serde_json::json!({"routeKind": "switch", "selectedHandle": "free"}),
        );
        assert_eq!(
            fallback
                .iter()
                .map(|edge| edge.id.as_str())
                .collect::<Vec<_>>(),
            vec!["fallback"]
        );

        let ordinary = automation_active_outgoing_edges(
            &edges,
            "branch",
            &serde_json::json!({"selectedHandles": ["success"]}),
        );
        assert_eq!(
            ordinary
                .iter()
                .map(|edge| edge.id.as_str())
                .collect::<Vec<_>>(),
            vec!["success"]
        );
    }
}
