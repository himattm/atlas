use atlas_repo::Graph;
use atlas_schemas::{AtlasResult, GraphContext, NavigationEdge, Route, ScreenNode};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone)]
pub struct RoutePlan {
    pub status: String,
    pub target: String,
    pub start_screen: Option<String>,
    pub route: Option<Route>,
    pub edges: Vec<NavigationEdge>,
    pub preferred_path_used: bool,
    pub graph_fallback_used: bool,
    pub context_mismatches: Vec<String>,
    pub reason: Option<String>,
}

impl RoutePlan {
    pub fn to_result(&self) -> AtlasResult {
        if !self.context_mismatches.is_empty() {
            return AtlasResult::context_mismatch(self.context_mismatches.clone());
        }
        AtlasResult {
            schema_version: atlas_schemas::RESULT_SCHEMA_VERSION.to_string(),
            status: self.status.clone(),
            summary: self.reason.clone(),
            data: json!({
                "target": self.target,
                "start_screen": self.start_screen,
                "edge_ids": self.edges.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>(),
                "preferred_path_used": self.preferred_path_used,
                "graph_fallback_used": self.graph_fallback_used,
                "route_confidence": if self.edges.is_empty() { 0.0 } else { 0.75 }
            }),
            recommended_action: None,
        }
    }
}

pub fn resolve_route(
    graph: &Graph,
    target: &str,
    current_screen: Option<&str>,
    context: &GraphContext,
) -> RoutePlan {
    let route = lookup_route(&graph.routes, &graph.screens, target);
    let target_screen = route
        .as_ref()
        .and_then(Route::target_screen)
        .map(str::to_string)
        .or_else(|| find_target_screen(&graph.screens, target));

    let Some(target_screen) = target_screen else {
        return RoutePlan {
            status: "not_found".to_string(),
            target: target.to_string(),
            start_screen: current_screen.map(str::to_string),
            route,
            edges: vec![],
            preferred_path_used: false,
            graph_fallback_used: false,
            context_mismatches: vec![],
            reason: Some("no matching route or screen".to_string()),
        };
    };

    let start_screen = current_screen.map(str::to_string).or_else(|| {
        route
            .as_ref()
            .and_then(Route::start_screen)
            .map(str::to_string)
    });

    let Some(start_screen) = start_screen else {
        return RoutePlan {
            status: "not_found".to_string(),
            target: target_screen,
            start_screen: None,
            route,
            edges: vec![],
            preferred_path_used: false,
            graph_fallback_used: false,
            context_mismatches: vec![],
            reason: Some("no start screen available".to_string()),
        };
    };

    if let Some(route) = &route {
        let mismatches = context.mismatches(&route.combined_context_guard());
        if !mismatches.is_empty() {
            return RoutePlan {
                status: "context_mismatch".to_string(),
                target: target_screen,
                start_screen: Some(start_screen),
                route: Some(route.clone()),
                edges: vec![],
                preferred_path_used: false,
                graph_fallback_used: false,
                context_mismatches: mismatches,
                reason: Some("route context guard mismatch".to_string()),
            };
        }

        if !route.preferred_edge_ids.is_empty() {
            let mut preferred = Vec::new();
            let mut missing_or_mismatch = None;
            for edge_id in &route.preferred_edge_ids {
                match graph.edges.get(edge_id) {
                    Some(edge) if context.satisfies(&edge.context_guard) => {
                        preferred.push(edge.clone())
                    }
                    Some(edge) => {
                        missing_or_mismatch = Some(context.mismatches(&edge.context_guard));
                        break;
                    }
                    None => break,
                }
            }
            if preferred.len() == route.preferred_edge_ids.len()
                && preferred.last().map(|edge| edge.to_screen.as_str())
                    == Some(target_screen.as_str())
            {
                return RoutePlan {
                    status: "ok".to_string(),
                    target: target_screen,
                    start_screen: Some(start_screen),
                    route: Some(route.clone()),
                    edges: preferred,
                    preferred_path_used: true,
                    graph_fallback_used: false,
                    context_mismatches: vec![],
                    reason: None,
                };
            }
            if let Some(mismatches) = missing_or_mismatch {
                return RoutePlan {
                    status: "context_mismatch".to_string(),
                    target: target_screen,
                    start_screen: Some(start_screen),
                    route: Some(route.clone()),
                    edges: vec![],
                    preferred_path_used: false,
                    graph_fallback_used: false,
                    context_mismatches: mismatches,
                    reason: Some("preferred path context guard mismatch".to_string()),
                };
            }
            if !route.allow_graph_fallback {
                return RoutePlan {
                    status: "route_broken".to_string(),
                    target: target_screen,
                    start_screen: Some(start_screen),
                    route: Some(route.clone()),
                    edges: vec![],
                    preferred_path_used: false,
                    graph_fallback_used: false,
                    context_mismatches: vec![],
                    reason: Some(
                        "preferred path unavailable and graph fallback disabled".to_string(),
                    ),
                };
            }
        }
    }

    let fallback = graph_fallback(&graph.edges, &start_screen, &target_screen, context);
    if let Some(edges) = fallback {
        RoutePlan {
            status: "ok".to_string(),
            target: target_screen,
            start_screen: Some(start_screen),
            route,
            edges,
            preferred_path_used: false,
            graph_fallback_used: true,
            context_mismatches: vec![],
            reason: Some("graph path found".to_string()),
        }
    } else {
        RoutePlan {
            status: "route_broken".to_string(),
            target: target_screen,
            start_screen: Some(start_screen),
            route,
            edges: vec![],
            preferred_path_used: false,
            graph_fallback_used: false,
            context_mismatches: vec![],
            reason: Some("no graph path reaches target".to_string()),
        }
    }
}

fn lookup_route(
    routes: &BTreeMap<String, Route>,
    screens: &BTreeMap<String, ScreenNode>,
    target: &str,
) -> Option<Route> {
    let needle = target.to_lowercase();
    for route in routes.values() {
        if route.name.to_lowercase() == needle
            || route
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase() == needle)
            || route.intent.as_ref().map(|intent| intent.to_lowercase()) == Some(needle.clone())
        {
            return Some(route.clone());
        }
    }
    let screen = find_target_screen(screens, target)?;
    routes
        .values()
        .find(|route| route.target_screen() == Some(screen.as_str()))
        .cloned()
}

fn find_target_screen(screens: &BTreeMap<String, ScreenNode>, target: &str) -> Option<String> {
    let needle = target.to_lowercase();
    screens.values().find_map(|screen| {
        if screen.id.to_lowercase() == needle
            || screen.name.to_lowercase() == needle
            || screen
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase() == needle)
        {
            Some(screen.name.clone())
        } else {
            None
        }
    })
}

fn graph_fallback(
    edges: &BTreeMap<String, NavigationEdge>,
    start: &str,
    target: &str,
    context: &GraphContext,
) -> Option<Vec<NavigationEdge>> {
    let mut queue = VecDeque::from([(start.to_string(), Vec::<NavigationEdge>::new())]);
    let mut visited = vec![start.to_string()];
    while let Some((screen, path)) = queue.pop_front() {
        if screen == target {
            return Some(path);
        }
        for edge in edges.values().filter(|edge| edge.from_screen == screen) {
            if !context.satisfies(&edge.context_guard) || visited.contains(&edge.to_screen) {
                continue;
            }
            let mut next_path = path.clone();
            next_path.push(edge.clone());
            visited.push(edge.to_screen.clone());
            queue.push_back((edge.to_screen.clone(), next_path));
        }
    }
    None
}

pub fn exit_code_for_status(status: &str) -> i32 {
    match status {
        "ok" | "passed" => 0,
        "changed_requires_review" => 1,
        "route_broken" => 3,
        "selector_drift" => 4,
        "screen_unknown" | "not_found" => 5,
        "environment_error" => 6,
        "config_error" => 7,
        "context_mismatch" => 8,
        _ => 2,
    }
}

pub fn route_result_json(plan: RoutePlan) -> Value {
    serde_json::to_value(plan.to_result()).expect("route result JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_schemas::{SelectorCandidate, TapRecipe};

    fn edge(id: &str, from: &str, to: &str) -> NavigationEdge {
        NavigationEdge {
            schema_version: atlas_schemas::EDGE_SCHEMA_VERSION.to_string(),
            id: id.to_string(),
            from_screen: from.to_string(),
            to_screen: to.to_string(),
            action: TapRecipe {
                kind: "tap".to_string(),
                description: None,
                selector_candidates: vec![SelectorCandidate {
                    kind: "test_tag".to_string(),
                    value: Some("next".to_string()),
                    score: Some(0.9),
                }],
                tap_cache: None,
            },
            intent: None,
            context_guard: BTreeMap::new(),
            expectations: vec![],
            learned_from: Value::Null,
            confidence_model: Value::Null,
        }
    }

    #[test]
    fn resolves_graph_fallback() {
        let graph = Graph {
            screens: BTreeMap::from([(
                "screen_article".to_string(),
                ScreenNode {
                    schema_version: atlas_schemas::SCREEN_SCHEMA_VERSION.to_string(),
                    id: "screen_article".to_string(),
                    name: "article".to_string(),
                    identity_hash: "sha256:x".to_string(),
                    context_guard: BTreeMap::new(),
                    aliases: vec![],
                    match_profile: Value::Null,
                    checks: vec![],
                    source: Value::Null,
                    normalized: None,
                },
            )]),
            edges: BTreeMap::from([
                (
                    "edge_home_feed".to_string(),
                    edge("edge_home_feed", "home", "feed"),
                ),
                (
                    "edge_feed_article".to_string(),
                    edge("edge_feed_article", "feed", "article"),
                ),
            ]),
            routes: BTreeMap::from([(
                "read-article".to_string(),
                Route {
                    schema_version: atlas_schemas::ROUTE_SCHEMA_VERSION.to_string(),
                    name: "read-article".to_string(),
                    target: json!({"screen": "article"}),
                    intent: None,
                    start: json!({"screen": "home"}),
                    preferred_edge_ids: vec!["missing".to_string()],
                    allow_graph_fallback: true,
                    path_constraints: Value::Null,
                    checks: vec![],
                    triggers: vec![],
                    aliases: vec![],
                    context_guard: BTreeMap::new(),
                },
            )]),
        };
        let plan = resolve_route(
            &graph,
            "read-article",
            Some("home"),
            &GraphContext::default(),
        );
        assert_eq!(plan.status, "ok");
        assert!(plan.graph_fallback_used);
        assert_eq!(plan.edges.len(), 2);
    }
}
