use crate::contracts::load_apoc_procedure_manifest;
use crate::errors::{ErrorCode, GraphDbError};
use crate::graph::{GraphNode, InMemoryGraphStore, Properties, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryResult {
    Nodes(Vec<GraphNode>),
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
    },
    Ack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowComparisonStrategy {
    Ordered,
    Multiset,
}

pub fn resolve_row_comparison_strategy(query: &str) -> RowComparisonStrategy {
    if query.contains("ORDER BY") {
        RowComparisonStrategy::Ordered
    } else {
        RowComparisonStrategy::Multiset
    }
}

pub fn execute_query(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    let mut q = query.trim().to_string();
    if (q.starts_with("MATCH ") || q.starts_with("OPTIONAL MATCH "))
        && q.contains(" WITH n,r,m RETURN n,r,m")
    {
        q = q.replacen(" WITH n,r,m RETURN n,r,m", " RETURN n,r,m", 1);
    }
    if (q.starts_with("MATCH ") || q.starts_with("OPTIONAL MATCH ")) && q.contains(" WITH ") {
        q = normalize_with_query(&q)?;
    }

    if q.starts_with("MATCH ") {
        if q.contains("-[r]->(") || q.contains("-[r]-(") {
            return execute_match_relationship(store, &q, false);
        }
        return execute_match(store, &q);
    }
    if q.starts_with("OPTIONAL MATCH ") {
        if q.contains("-[r]->(") || q.contains("-[r]-(") {
            return execute_match_relationship(store, &q, true);
        }
        return execute_optional_match(store, &q);
    }
    if q.starts_with("UNWIND ") {
        return execute_unwind(&q);
    }
    if q.starts_with("CREATE ") {
        return execute_create(store, &q);
    }
    if q.starts_with("MERGE ") {
        return execute_merge(store, &q);
    }
    if q.starts_with("DELETE ") {
        return execute_delete(store, &q);
    }
    if q.starts_with("SET ") {
        return execute_set(store, &q);
    }
    if q.starts_with("REMOVE ") {
        return execute_remove(store, &q);
    }
    if q.starts_with("CALL ") {
        return execute_call(store, &q);
    }

    Err(GraphDbError::new(
        ErrorCode::UnsupportedFeature,
        "unsupported cypher clause for openCypher 9 profile",
    )
    .with_detail("unsupported_clause", q.split_whitespace().next().unwrap_or("UNKNOWN")))
}

fn execute_match(store: &InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    let compact = query.trim();
    if !compact.starts_with("MATCH ") || !compact.contains("RETURN n") {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported MATCH form")
                .with_detail("unsupported_clause", "MATCH"),
        );
    }

    let (label_filter, id_filter) = parse_match_filters(compact)?;
    let mut nodes = store.list_nodes();
    if let Some(label) = label_filter {
        nodes.retain(|node| node.labels.iter().any(|l| l == &label));
    }
    if let Some(id) = id_filter {
        nodes.retain(|node| node.id == id);
    }

    nodes = apply_return_modifiers(nodes, compact)?;
    Ok(QueryResult::Nodes(nodes))
}

fn execute_optional_match(store: &InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    let rewritten = query.replacen("OPTIONAL MATCH", "MATCH", 1);
    execute_match(store, &rewritten)
}

fn execute_match_relationship(
    store: &InMemoryGraphStore,
    query: &str,
    optional: bool,
) -> Result<QueryResult, GraphDbError> {
    if !query.contains("RETURN n,r,m") {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "relationship query must return n,r,m")
                .with_detail("unsupported_clause", "MATCH"),
        );
    }
    let directed = if query.contains("-[r]->(") {
        true
    } else if query.contains("-[r]-(") {
        false
    } else {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported relationship pattern")
                .with_detail("unsupported_clause", "MATCH"),
        );
    };
    let weight_filter = parse_relationship_weight_filter(query)?;
    let mut rows = build_relationship_rows(store, directed, optional, weight_filter)?;
    rows = apply_table_modifiers(rows, query)?;
    Ok(QueryResult::Table {
        columns: vec!["n".to_string(), "r".to_string(), "m".to_string()],
        rows,
    })
}

fn parse_relationship_weight_filter(query: &str) -> Result<Option<f64>, GraphDbError> {
    let Some(where_idx) = query.find("WHERE ") else {
        return Ok(None);
    };
    let after_where = &query[where_idx + 6..];
    let before_return = after_where
        .split("RETURN")
        .next()
        .unwrap_or_default()
        .trim();
    if before_return.is_empty() {
        return Ok(None);
    }
    let marker = "r.weight >";
    if !before_return.starts_with(marker) {
        return Err(
            GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "WHERE in relationship query supports only r.weight > <num>",
            )
            .with_detail("unsupported_clause", "WHERE"),
        );
    }
    let raw = before_return[marker.len()..].trim();
    let value = raw
        .parse::<f64>()
        .map_err(|_| syntax_error("invalid r.weight numeric value"))?;
    Ok(Some(value))
}

fn build_relationship_rows(
    store: &InMemoryGraphStore,
    directed: bool,
    optional: bool,
    weight_filter: Option<f64>,
) -> Result<Vec<Vec<Value>>, GraphDbError> {
    let edges = store.list_edges();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut matched_source_ids = std::collections::HashSet::new();
    let mut matched_any_ids = std::collections::HashSet::new();

    for edge in edges {
        if let Some(min_weight) = weight_filter {
            let w = edge
                .properties
                .get("weight")
                .and_then(|v| match v {
                    Value::Int64(n) => Some(*n as f64),
                    Value::Float64(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(0.0);
            if w <= min_weight {
                continue;
            }
        }
        let source = store.get_node(&edge.from).ok_or_else(|| {
            GraphDbError::new(ErrorCode::ReferentialIntegrityViolation, "missing edge source node")
        })?;
        let target = store.get_node(&edge.to).ok_or_else(|| {
            GraphDbError::new(ErrorCode::ReferentialIntegrityViolation, "missing edge target node")
        })?;
        rows.push(vec![
            Value::String(source.id.clone()),
            Value::String(edge.id.clone()),
            Value::String(target.id.clone()),
        ]);
        matched_source_ids.insert(source.id.clone());
        matched_any_ids.insert(source.id.clone());
        matched_any_ids.insert(target.id.clone());

        if !directed {
            rows.push(vec![
                Value::String(target.id.clone()),
                Value::String(edge.id.clone()),
                Value::String(source.id.clone()),
            ]);
        }
    }

    if optional {
        for node in store.list_nodes() {
            let is_matched = if directed {
                matched_source_ids.contains(&node.id)
            } else {
                matched_any_ids.contains(&node.id)
            };
            if !is_matched {
                rows.push(vec![
                    Value::String(node.id),
                    Value::String("NULL".to_string()),
                    Value::String("NULL".to_string()),
                ]);
            }
        }
    }
    Ok(rows)
}

fn apply_table_modifiers(
    mut rows: Vec<Vec<Value>>,
    query: &str,
) -> Result<Vec<Vec<Value>>, GraphDbError> {
    let (order_by, skip, limit) = parse_return_modifiers(query)?;
    if order_by {
        rows.sort_by(|a, b| {
            let ak = a.first();
            let bk = b.first();
            let as_id = match ak {
                Some(Value::String(v)) => v.as_str(),
                _ => "",
            };
            let bs_id = match bk {
                Some(Value::String(v)) => v.as_str(),
                _ => "",
            };
            as_id.cmp(bs_id)
        });
    }
    if let Some(skip) = skip {
        if skip >= rows.len() {
            rows.clear();
        } else {
            rows = rows.split_off(skip);
        }
    }
    if let Some(limit) = limit {
        rows.truncate(limit);
    }
    Ok(rows)
}

fn execute_call(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    let call = query
        .strip_prefix("CALL ")
        .ok_or_else(|| syntax_error("invalid CALL syntax"))?
        .trim();
    let open = call.find('(').ok_or_else(|| syntax_error("CALL requires ("))?;
    let close = call.rfind(')').ok_or_else(|| syntax_error("CALL requires )"))?;
    if close <= open {
        return Err(syntax_error("invalid CALL argument list"));
    }
    let name = call[..open].trim();
    let args_raw = call[open + 1..close].trim();

    let manifest = load_apoc_procedure_manifest();
    if !manifest.allowed_procedures.iter().any(|p| p.name == name) {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, format!("unsupported procedure: {name}"))
                .with_detail("unsupported_clause", "CALL"),
        );
    }

    match name {
        "apoc.meta.schema" => Ok(QueryResult::Table {
            columns: vec!["nodes".to_string(), "relationships".to_string()],
            rows: vec![vec![
                Value::Int64(store.node_count() as i64),
                Value::Int64(store.edge_count() as i64),
            ]],
        }),
        "apoc.coll.toSet" => {
            let values = parse_list_values(args_raw)?;
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for v in values {
                let key = format!("{v:?}");
                if seen.insert(key) {
                    out.push(v);
                }
            }
            Ok(QueryResult::Table {
                columns: vec!["values".to_string()],
                rows: vec![out],
            })
        }
        "apoc.text.join" => {
            let parts = split_call_args(args_raw);
            if parts.len() != 2 {
                return Err(GraphDbError::new(ErrorCode::InvalidArgument, "apoc.text.join requires values, delimiter"));
            }
            let values = parse_list_values(parts[0])?;
            let delimiter = parse_value(parts[1])?;
            let delim = match delimiter {
                Value::String(v) => v,
                _ => return Err(GraphDbError::new(ErrorCode::InvalidArgument, "delimiter must be string")),
            };
            let joined = values
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Int64(n) => n.to_string(),
                    Value::Float64(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Bytes(_) => "<bytes>".to_string(),
                })
                .collect::<Vec<_>>()
                .join(&delim);
            Ok(QueryResult::Table {
                columns: vec!["value".to_string()],
                rows: vec![vec![Value::String(joined)]],
            })
        }
        "apoc.refactor.rename.label" => {
            let parts = split_call_args(args_raw);
            if parts.len() != 2 {
                return Err(GraphDbError::new(
                    ErrorCode::InvalidArgument,
                    "apoc.refactor.rename.label requires from,to",
                ));
            }
            let from = match parse_value(parts[0])? {
                Value::String(v) => v,
                _ => return Err(GraphDbError::new(ErrorCode::InvalidArgument, "from must be string")),
            };
            let to = match parse_value(parts[1])? {
                Value::String(v) => v,
                _ => return Err(GraphDbError::new(ErrorCode::InvalidArgument, "to must be string")),
            };
            if from.trim().is_empty() || to.trim().is_empty() {
                return Err(GraphDbError::new(ErrorCode::InvalidArgument, "from/to must not be empty"));
            }
            let updated = store.rename_label(&from, &to);
            Ok(QueryResult::Table {
                columns: vec!["updatedNodeCount".to_string()],
                rows: vec![vec![Value::Int64(updated as i64)]],
            })
        }
        _ => Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, format!("unsupported procedure: {name}"))
                .with_detail("unsupported_clause", "CALL"),
        ),
    }
}

fn split_call_args(raw: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (idx, ch) in raw.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(raw[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < raw.len() {
        out.push(raw[start..].trim());
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn execute_create(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    if !query.starts_with("CREATE (") || !query.ends_with(')') {
        return Err(syntax_error("invalid CREATE syntax"));
    }
    let (label, props) = parse_create_or_merge_payload(query, "CREATE ")?;
    store.create_node(vec![label], props);
    Ok(QueryResult::Ack)
}

fn execute_merge(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    if !query.starts_with("MERGE (") {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported MERGE form")
                .with_detail("unsupported_clause", "MERGE"),
        );
    }
    let (pattern_part, tail) = split_pattern_and_tail(query, "MERGE ")?;
    let (label, props) = parse_create_or_merge_payload(pattern_part, "MERGE ")?;

    if let Some(existing) = store.find_node_by_label_and_props(Some(&label), &props) {
        if let Some((key, value)) = parse_merge_set_clause(tail, "ON MATCH SET")? {
            let node = store.get_node_mut(&existing.id).ok_or_else(|| {
                GraphDbError::new(ErrorCode::ReferentialIntegrityViolation, "MERGE target missing")
            })?;
            node.properties.insert(key, value);
        }
    } else {
        let created = store.create_node(vec![label], props);
        if let Some((key, value)) = parse_merge_set_clause(tail, "ON CREATE SET")? {
            let node = store.get_node_mut(&created.id).ok_or_else(|| {
                GraphDbError::new(ErrorCode::ReferentialIntegrityViolation, "MERGE create target missing")
            })?;
            node.properties.insert(key, value);
        }
    }

    Ok(QueryResult::Ack)
}

fn execute_set(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    // SET NODE <id> <key>=<value>
    let content = query.trim_start_matches("SET ").trim();
    if !content.starts_with("NODE ") {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported SET form")
                .with_detail("unsupported_clause", "SET"),
        );
    }
    let mut parts = content.splitn(3, ' ');
    let _node_kw = parts.next();
    let node_id = parts
        .next()
        .ok_or_else(|| GraphDbError::new(ErrorCode::UnsupportedFeature, "SET NODE requires node id"))?;
    let assignment = parts.next().ok_or_else(|| {
        GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "SET NODE requires key=value assignment",
        )
    })?;
    let (key, value) = parse_assignment(assignment)?;
    let node = store.get_node_mut(node_id).ok_or_else(|| {
        GraphDbError::new(
            ErrorCode::ReferentialIntegrityViolation,
            "SET NODE target does not exist",
        )
    })?;
    node.properties.insert(key, value);
    Ok(QueryResult::Ack)
}

fn execute_remove(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    // REMOVE NODE <id> <key>
    let content = query.trim_start_matches("REMOVE ").trim();
    if !content.starts_with("NODE ") {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported REMOVE form")
                .with_detail("unsupported_clause", "REMOVE"),
        );
    }
    let mut parts = content.splitn(3, ' ');
    let _node_kw = parts.next();
    let node_id = parts
        .next()
        .ok_or_else(|| GraphDbError::new(ErrorCode::UnsupportedFeature, "REMOVE NODE requires node id"))?;
    let key = parts.next().ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "REMOVE NODE requires property key")
    })?;
    let key = key.trim().strip_prefix("n.").unwrap_or(key);
    let node = store.get_node_mut(node_id).ok_or_else(|| {
        GraphDbError::new(
            ErrorCode::ReferentialIntegrityViolation,
            "REMOVE NODE target does not exist",
        )
    })?;
    node.properties.remove(key);
    Ok(QueryResult::Ack)
}

fn execute_delete(store: &mut InMemoryGraphStore, query: &str) -> Result<QueryResult, GraphDbError> {
    // DELETE NODE <id>
    // DELETE NODE <id> DETACH
    let parts: Vec<&str> = query.split_whitespace().collect();
    if parts.len() < 3 || parts[0] != "DELETE" || parts[1] != "NODE" {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported DELETE form")
                .with_detail("unsupported_clause", "DELETE"),
        );
    }
    let node_id = parts[2];
    let detach = parts.get(3).map(|v| *v == "DETACH").unwrap_or(false);
    if !store.has_node(node_id) {
        return Ok(QueryResult::Ack);
    }

    let touching = store.edges_touching_node(node_id);
    if !touching.is_empty() && !detach {
        return Err(GraphDbError::new(
            ErrorCode::ReferentialIntegrityViolation,
            "node is still referenced by edges; use DETACH",
        ));
    }

    if detach {
        store.delete_node_detach(node_id);
    } else {
        store.delete_node(node_id);
    }
    Ok(QueryResult::Ack)
}

fn execute_unwind(query: &str) -> Result<QueryResult, GraphDbError> {
    // UNWIND [1,2,3] AS x RETURN count(x)|collect(x)|sum(x)|avg(x)|min(x)|max(x)|x
    let content = query
        .trim()
        .strip_prefix("UNWIND ")
        .ok_or_else(|| GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported UNWIND form"))?;
    let as_idx = content.find(" AS ").ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "UNWIND requires AS variable")
    })?;
    let list_part = content[..as_idx].trim();
    let rest = content[as_idx + 4..].trim();
    let return_idx = rest.find(" RETURN ").ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "UNWIND requires RETURN")
    })?;
    let var = rest[..return_idx].trim();
    let projection = rest[return_idx + 8..].trim();
    let values = parse_list_values(list_part)?;

    if projection == format!("count({var})") {
        return Ok(QueryResult::Table {
            columns: vec!["count".to_string()],
            rows: vec![vec![Value::Int64(values.len() as i64)]],
        });
    }
    if projection == format!("collect({var})") {
        return Ok(QueryResult::Table {
            columns: vec!["collect".to_string()],
            rows: vec![values],
        });
    }

    let numeric = values
        .iter()
        .map(value_as_f64)
        .collect::<Result<Vec<_>, _>>()?;
    if projection == format!("sum({var})") {
        return Ok(QueryResult::Table {
            columns: vec!["sum".to_string()],
            rows: vec![vec![Value::Float64(numeric.iter().sum())]],
        });
    }
    if projection == format!("avg({var})") {
        let avg = if numeric.is_empty() {
            0.0
        } else {
            numeric.iter().sum::<f64>() / numeric.len() as f64
        };
        return Ok(QueryResult::Table {
            columns: vec!["avg".to_string()],
            rows: vec![vec![Value::Float64(avg)]],
        });
    }
    if projection == format!("min({var})") {
        let min = numeric.iter().fold(f64::INFINITY, |acc, v| acc.min(*v));
        return Ok(QueryResult::Table {
            columns: vec!["min".to_string()],
            rows: vec![vec![Value::Float64(if min.is_finite() { min } else { 0.0 })]],
        });
    }
    if projection == format!("max({var})") {
        let max = numeric.iter().fold(f64::NEG_INFINITY, |acc, v| acc.max(*v));
        return Ok(QueryResult::Table {
            columns: vec!["max".to_string()],
            rows: vec![vec![Value::Float64(if max.is_finite() { max } else { 0.0 })]],
        });
    }
    if projection == var {
        return Ok(QueryResult::Table {
            columns: vec![var.to_string()],
            rows: values.into_iter().map(|v| vec![v]).collect(),
        });
    }
    Err(
        GraphDbError::new(ErrorCode::UnsupportedFeature, "unsupported UNWIND projection")
            .with_detail("unsupported_clause", "UNWIND"),
    )
}

fn normalize_with_query(query: &str) -> Result<String, GraphDbError> {
    // Supported:
    // MATCH (...) WITH n RETURN n
    // MATCH (...) WITH n AS x RETURN x [ORDER BY ... SKIP ... LIMIT ...]
    let with_idx = query.find(" WITH ").ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "WITH clause missing")
    })?;
    let left = query[..with_idx].trim();
    let right = query[with_idx + 6..].trim();
    let return_idx = right.find(" RETURN ").ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "WITH requires RETURN")
    })?;
    let projection_ctx = right[..return_idx].trim();
    let return_expr = right[return_idx + 8..].trim();

    let (source_var, projected_var) = parse_with_projection(projection_ctx)?;
    if source_var != "n" {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "WITH source variable out of scope")
                .with_detail("unsupported_clause", "WITH"),
        );
    }

    if return_expr == source_var && projected_var != source_var {
        return Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "WITH alias scope violation")
                .with_detail("unsupported_clause", "WITH"),
        );
    }

    let rewritten_return = if return_expr.starts_with(projected_var) {
        return_expr.replacen(projected_var, "n", 1)
    } else {
        return Err(
            GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "RETURN expression not available in WITH scope",
            )
            .with_detail("unsupported_clause", "WITH"),
        );
    };
    Ok(format!("{left} RETURN {rewritten_return}"))
}

fn parse_with_projection(projection: &str) -> Result<(&str, &str), GraphDbError> {
    if let Some(as_idx) = projection.find(" AS ") {
        let source = projection[..as_idx].trim();
        let alias = projection[as_idx + 4..].trim();
        if source.is_empty() || alias.is_empty() {
            return Err(syntax_error("invalid WITH alias syntax"));
        }
        return Ok((source, alias));
    }
    if projection.is_empty() {
        return Err(syntax_error("WITH projection is empty"));
    }
    Ok((projection, projection))
}

fn split_pattern_and_tail<'a>(query: &'a str, prefix: &str) -> Result<(&'a str, &'a str), GraphDbError> {
    let trimmed = query.trim();
    let body = trimmed
        .strip_prefix(prefix)
        .ok_or_else(|| GraphDbError::new(ErrorCode::UnsupportedFeature, "missing clause prefix"))?;
    let mut depth = 0usize;
    for (idx, ch) in body.char_indices() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let full_pattern_end = prefix.len() + idx + 1;
                return Ok((&trimmed[..full_pattern_end], &trimmed[full_pattern_end..]));
            }
        }
    }
    Err(syntax_error("invalid node pattern"))
}

fn parse_merge_set_clause(tail: &str, marker: &str) -> Result<Option<(String, Value)>, GraphDbError> {
    let trimmed = tail.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some(pos) = trimmed.find(marker) {
        let after = trimmed[pos + marker.len()..].trim();
        let until_next = if let Some(next) = after.find(" ON ") {
            &after[..next]
        } else {
            after
        };
        return parse_assignment(until_next).map(Some);
    }
    Ok(None)
}

fn parse_create_or_merge_payload(
    query: &str,
    prefix: &str,
) -> Result<(String, Properties), GraphDbError> {
    let inner = query.trim_start_matches(prefix).trim();
    let inner = inner
        .strip_prefix('(')
        .and_then(|v| v.strip_suffix(')'))
        .ok_or_else(|| syntax_error("invalid node pattern"))?;
    let colon = inner
        .find(':')
        .ok_or_else(|| GraphDbError::new(ErrorCode::UnsupportedFeature, "label is required in node pattern"))?;
    let after_colon = &inner[colon + 1..];
    let (label, props) = if let Some(brace) = after_colon.find('{') {
        let label = after_colon[..brace].trim().to_string();
        let props_part = after_colon[brace..].trim();
        let props = parse_properties(props_part)?;
        (label, props)
    } else {
        (after_colon.trim().to_string(), Properties::new())
    };
    Ok((label, props))
}

fn parse_properties(props_part: &str) -> Result<Properties, GraphDbError> {
    let raw = props_part
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .ok_or_else(|| syntax_error("invalid properties object"))?;
    if raw.trim().is_empty() {
        return Ok(Properties::new());
    }
    let mut props = Properties::new();
    for entry in raw.split(',') {
        let (key, value) = parse_assignment(entry.trim())?;
        props.insert(key, value);
    }
    Ok(props)
}

fn parse_assignment(assignment: &str) -> Result<(String, Value), GraphDbError> {
    let (left, right) = if let Some(idx) = assignment.find('=') {
        (&assignment[..idx], &assignment[idx + 1..])
    } else if let Some(idx) = assignment.find(':') {
        (&assignment[..idx], &assignment[idx + 1..])
    } else {
        return Err(syntax_error("assignment must be key=value or key:value"));
    };
    let key = left.trim().strip_prefix("n.").unwrap_or(left.trim()).to_string();
    let value = parse_value(right.trim())?;
    Ok((key, value))
}

fn parse_list_values(raw: &str) -> Result<Vec<Value>, GraphDbError> {
    let inner = raw
        .trim()
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .ok_or_else(|| syntax_error("invalid UNWIND list"))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|part| parse_value(part.trim()))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_value(raw: &str) -> Result<Value, GraphDbError> {
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Ok(Value::String(raw[1..raw.len() - 1].to_string()));
    }
    if raw.eq_ignore_ascii_case("true") {
        return Ok(Value::Bool(true));
    }
    if raw.eq_ignore_ascii_case("false") {
        return Ok(Value::Bool(false));
    }
    if let Ok(v) = raw.parse::<i64>() {
        return Ok(Value::Int64(v));
    }
    if let Ok(v) = raw.parse::<f64>() {
        return Ok(Value::Float64(v));
    }
    Err(syntax_error("unsupported literal value"))
}

fn value_as_f64(value: &Value) -> Result<f64, GraphDbError> {
    match value {
        Value::Int64(v) => Ok(*v as f64),
        Value::Float64(v) => Ok(*v),
        _ => Err(
            GraphDbError::new(ErrorCode::UnsupportedFeature, "aggregation requires numeric values")
                .with_detail("unsupported_clause", "UNWIND"),
        ),
    }
}

fn apply_return_modifiers(mut nodes: Vec<GraphNode>, query: &str) -> Result<Vec<GraphNode>, GraphDbError> {
    let (_order_by, skip, limit) = parse_return_modifiers(query)?;
    if let Some(skip) = skip {
        if skip >= nodes.len() {
            nodes.clear();
        } else {
            nodes = nodes.split_off(skip);
        }
    }
    if let Some(limit) = limit {
        nodes.truncate(limit);
    }
    Ok(nodes)
}

fn parse_return_modifiers(query: &str) -> Result<(bool, Option<usize>, Option<usize>), GraphDbError> {
    let mut order_by = false;
    let mut skip = None;
    let mut limit = None;
    if let Some(idx) = query.find("ORDER BY") {
        let tail = query[idx..].trim();
        order_by = true;
        if !tail.starts_with("ORDER BY n.id") {
            return Err(
                GraphDbError::new(ErrorCode::UnsupportedFeature, "only ORDER BY n.id is supported")
                    .with_detail("unsupported_clause", "ORDER BY"),
            );
        }
    }
    if let Some(idx) = query.find("SKIP ") {
        let tail = &query[idx + 5..];
        let num = tail
            .split_whitespace()
            .next()
            .ok_or_else(|| syntax_error("SKIP requires number"))?;
        skip = Some(
            num.parse::<usize>()
                .map_err(|_| syntax_error("invalid SKIP number"))?,
        );
    }
    if let Some(idx) = query.find("LIMIT ") {
        let tail = &query[idx + 6..];
        let num = tail
            .split_whitespace()
            .next()
            .ok_or_else(|| syntax_error("LIMIT requires number"))?;
        limit = Some(
            num.parse::<usize>()
                .map_err(|_| syntax_error("invalid LIMIT number"))?,
        );
    }
    Ok((order_by, skip, limit))
}

fn parse_match_filters(query: &str) -> Result<(Option<String>, Option<String>), GraphDbError> {
    let after_match = query.trim_start_matches("MATCH ").trim();
    let pattern_end = after_match
        .find(')')
        .ok_or_else(|| syntax_error("invalid MATCH pattern"))?;
    let pattern = &after_match[..=pattern_end];
    let remainder = after_match[pattern_end + 1..].trim();
    let label = if let Some(colon) = pattern.find(':') {
        Some(pattern[colon + 1..pattern.len() - 1].trim().to_string())
    } else {
        None
    };
    if !remainder.starts_with("RETURN n") && !remainder.starts_with("WHERE ") {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "unsupported MATCH remainder",
        ));
    }

    if remainder.starts_with("RETURN n") {
        return Ok((label, None));
    }

    // WHERE n.id = 'n1' RETURN n
    let where_part = remainder.trim_start_matches("WHERE ").trim();
    let return_idx = where_part
        .find("RETURN n")
        .ok_or_else(|| syntax_error("WHERE must end with RETURN n"))?;
    let condition = where_part[..return_idx].trim();
    let mut split = condition.splitn(2, '=');
    let left = split.next().unwrap_or_default().trim();
    let right = split.next().ok_or_else(|| {
        GraphDbError::new(ErrorCode::UnsupportedFeature, "WHERE supports only equality")
    })?;
    if left != "n.id" {
        return Err(GraphDbError::new(
            ErrorCode::UnsupportedFeature,
            "WHERE currently supports n.id equality only",
        ));
    }
    let parsed = parse_value(right.trim())?;
    let id = match parsed {
        Value::String(id) => id,
        _ => {
            return Err(GraphDbError::new(
                ErrorCode::UnsupportedFeature,
                "n.id filter must be string",
            ))
        }
    };
    Ok((label, Some(id)))
}

fn syntax_error(message: &str) -> GraphDbError {
    GraphDbError::new(ErrorCode::UnsupportedFeature, message).with_detail("unsupported_clause", "SYNTAX")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::InMemoryGraphStore;

    #[test]
    fn executes_create_and_match() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");
        let result = execute_query(&mut store, "MATCH (n) RETURN n").expect("match");
        match result {
            QueryResult::Nodes(nodes) => assert_eq!(nodes.len(), 1),
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn rejects_unsupported_clause() {
        let mut store = InMemoryGraphStore::new();
        let err = execute_query(&mut store, "CALL db.labels()").expect_err("unsupported");
        assert_eq!(err.code, ErrorCode::UnsupportedFeature);
        let details = err.details.expect("details required");
        assert_eq!(details.get("unsupported_clause").map(String::as_str), Some("CALL"));
    }

    #[test]
    fn supports_relationship_traversal_directed_and_undirected() {
        let mut store = InMemoryGraphStore::new();
        let n1 = store.create_node(vec!["A".to_string()], Properties::new());
        let n2 = store.create_node(vec!["B".to_string()], Properties::new());
        let _ = store.create_edge(&n1.id, &n2.id, "REL".to_string(), Properties::new());

        let directed = execute_query(&mut store, "MATCH (n)-[r]->(m) RETURN n,r,m").expect("directed");
        match directed {
            QueryResult::Table { rows, .. } => assert_eq!(rows.len(), 1),
            _ => panic!("expected table"),
        }

        let undirected = execute_query(&mut store, "MATCH (n)-[r]-(m) RETURN n,r,m").expect("undirected");
        match undirected {
            QueryResult::Table { rows, .. } => assert_eq!(rows.len(), 2),
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn supports_relationship_optional_where_with_order_skip_limit() {
        let mut store = InMemoryGraphStore::new();
        let n1 = store.create_node(vec!["A".to_string()], Properties::new());
        let n2 = store.create_node(vec!["B".to_string()], Properties::new());
        let _n3 = store.create_node(vec!["C".to_string()], Properties::new());
        let mut edge_props = Properties::new();
        edge_props.insert("weight".to_string(), Value::Float64(0.9));
        let _ = store.create_edge(&n1.id, &n2.id, "REL".to_string(), edge_props);

        let filtered = execute_query(
            &mut store,
            "MATCH (n)-[r]->(m) WHERE r.weight > 0.5 WITH n,r,m RETURN n,r,m ORDER BY n.id SKIP 0 LIMIT 10",
        )
        .expect("filtered");
        match filtered {
            QueryResult::Table { rows, .. } => assert_eq!(rows.len(), 1),
            _ => panic!("expected table"),
        }

        let optional =
            execute_query(&mut store, "OPTIONAL MATCH (n)-[r]->(m) RETURN n,r,m").expect("optional");
        match optional {
            QueryResult::Table { rows, .. } => {
                assert!(rows.iter().any(|row| matches!(row.get(1), Some(Value::String(v)) if v == "NULL")));
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn supports_manifest_call_apoc_subset() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");

        let schema = execute_query(&mut store, "CALL apoc.meta.schema()").expect("meta schema");
        match schema {
            QueryResult::Table { columns, .. } => {
                assert_eq!(columns, vec!["nodes".to_string(), "relationships".to_string()])
            }
            _ => panic!("expected table"),
        }

        let to_set = execute_query(&mut store, "CALL apoc.coll.toSet([1,2,2,3])").expect("toSet");
        match to_set {
            QueryResult::Table { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 3);
            }
            _ => panic!("expected table"),
        }

        let join = execute_query(&mut store, "CALL apoc.text.join(['a','b'],'-')").expect("join");
        match join {
            QueryResult::Table { rows, .. } => {
                assert_eq!(rows[0][0], Value::String("a-b".to_string()));
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn call_side_effect_procedure_is_atomic_and_returns_fixed_error_code() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");
        let renamed =
            execute_query(&mut store, "CALL apoc.refactor.rename.label('Paper','Doc')").expect("rename");
        match renamed {
            QueryResult::Table { rows, .. } => {
                assert_eq!(rows[0][0], Value::Int64(1));
            }
            _ => panic!("expected table"),
        }

        let err = execute_query(&mut store, "CALL apoc.refactor.rename.label('','Doc')")
            .expect_err("invalid arguments");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn enforces_referential_integrity_on_delete() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Author)").expect("create 1");
        execute_query(&mut store, "CREATE (n:Paper)").expect("create 2");
        let _ = store.create_edge("n1", "n2", "WROTE".to_string(), Properties::new());

        let err = execute_query(&mut store, "DELETE NODE n1").expect_err("must fail");
        assert_eq!(err.code, ErrorCode::ReferentialIntegrityViolation);
        execute_query(&mut store, "DELETE NODE n1 DETACH").expect("detach delete");
    }

    #[test]
    fn supports_match_where_filter() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");
        execute_query(&mut store, "CREATE (n:Paper)").expect("create 2");
        let result = execute_query(&mut store, "MATCH (n) WHERE n.id='n2' RETURN n").expect("where");
        match result {
            QueryResult::Nodes(nodes) => {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].id, "n2");
            }
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn supports_optional_match_and_order_skip_limit() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create1");
        execute_query(&mut store, "CREATE (n:Paper)").expect("create2");
        execute_query(&mut store, "CREATE (n:Paper)").expect("create3");
        let result = execute_query(
            &mut store,
            "OPTIONAL MATCH (n:Paper) RETURN n ORDER BY n.id SKIP 1 LIMIT 1",
        )
        .expect("optional");
        match result {
            QueryResult::Nodes(nodes) => {
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].id, "n2");
            }
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn supports_with_passthrough() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");
        let result = execute_query(&mut store, "MATCH (n) WITH n RETURN n").expect("with");
        match result {
            QueryResult::Nodes(nodes) => assert_eq!(nodes.len(), 1),
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn rejects_with_alias_scope_violation() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper)").expect("create");
        let err = execute_query(&mut store, "MATCH (n) WITH n AS x RETURN n").expect_err("must fail");
        assert_eq!(err.code, ErrorCode::UnsupportedFeature);
        let details = err.details.expect("details");
        assert_eq!(details.get("unsupported_clause").map(String::as_str), Some("WITH"));
    }

    #[test]
    fn supports_merge_on_create_and_on_match_set() {
        let mut store = InMemoryGraphStore::new();
        execute_query(
            &mut store,
            "MERGE (n:Paper {title='A'}) ON CREATE SET n.status='new'",
        )
        .expect("merge create");
        execute_query(
            &mut store,
            "MERGE (n:Paper {title='A'}) ON MATCH SET n.status='existing'",
        )
        .expect("merge match");
        let result = execute_query(&mut store, "MATCH (n:Paper) RETURN n").expect("match");
        match result {
            QueryResult::Nodes(nodes) => {
                assert_eq!(nodes.len(), 1);
                let status = nodes[0].properties.get("status");
                assert!(matches!(status, Some(Value::String(v)) if v == "existing"));
            }
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn supports_remove_property() {
        let mut store = InMemoryGraphStore::new();
        execute_query(&mut store, "CREATE (n:Paper {title='A'})").expect("create");
        execute_query(&mut store, "REMOVE NODE n1 title").expect("remove");
        let result = execute_query(&mut store, "MATCH (n) WHERE n.id='n1' RETURN n").expect("match");
        match result {
            QueryResult::Nodes(nodes) => {
                assert!(!nodes[0].properties.contains_key("title"));
            }
            _ => panic!("expected nodes"),
        }
    }

    #[test]
    fn supports_unwind_all_aggregations() {
        let mut store = InMemoryGraphStore::new();
        let count = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN count(x)").expect("count");
        match count {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["count".to_string()]);
                assert_eq!(rows, vec![vec![Value::Int64(3)]]);
            }
            _ => panic!("expected table"),
        }
        let collect = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN collect(x)").expect("collect");
        match collect {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["collect".to_string()]);
                assert_eq!(
                    rows,
                    vec![vec![Value::Int64(1), Value::Int64(2), Value::Int64(3)]]
                );
            }
            _ => panic!("expected table"),
        }
        let sum = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN sum(x)").expect("sum");
        match sum {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["sum".to_string()]);
                assert_eq!(rows, vec![vec![Value::Float64(6.0)]]);
            }
            _ => panic!("expected table"),
        }
        let avg = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN avg(x)").expect("avg");
        match avg {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["avg".to_string()]);
                assert_eq!(rows, vec![vec![Value::Float64(2.0)]]);
            }
            _ => panic!("expected table"),
        }
        let min = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN min(x)").expect("min");
        match min {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["min".to_string()]);
                assert_eq!(rows, vec![vec![Value::Float64(1.0)]]);
            }
            _ => panic!("expected table"),
        }
        let max = execute_query(&mut store, "UNWIND [1,2,3] AS x RETURN max(x)").expect("max");
        match max {
            QueryResult::Table { columns, rows } => {
                assert_eq!(columns, vec!["max".to_string()]);
                assert_eq!(rows, vec![vec![Value::Float64(3.0)]]);
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn picks_row_comparison_strategy() {
        assert_eq!(
            resolve_row_comparison_strategy("MATCH (n) RETURN n ORDER BY n.id"),
            RowComparisonStrategy::Ordered
        );
        assert_eq!(
            resolve_row_comparison_strategy("MATCH (n) RETURN n"),
            RowComparisonStrategy::Multiset
        );
    }
}
