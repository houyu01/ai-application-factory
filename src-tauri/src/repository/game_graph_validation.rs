//! DAG integrity checks for manually edited interactive-game choice edges.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// Reject an edge when it would let a later video node reach its own predecessor.
pub(super) fn ensure_acyclic_edge(
    connection: &Connection,
    game_id: &str,
    ignored_edge_id: Option<&str>,
    source_node_id: &str,
    target_node_id: &str,
) -> AppResult<()> {
    if source_node_id == target_node_id {
        return Err(AppError::BadRequest("选项不能连接到自身节点".to_owned()));
    }
    let mut statement = connection
        .prepare("SELECT id,source_node_id,target_node_id FROM game_edges WHERE game_id=?1")?;
    let edges = statement
        .query_map([game_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut next_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for (edge_id, source, target) in edges {
        if Some(edge_id.as_str()) != ignored_edge_id {
            next_nodes.entry(source).or_default().push(target);
        }
    }
    let mut pending = vec![target_node_id.to_owned()];
    let mut visited = HashSet::new();
    while let Some(node_id) = pending.pop() {
        if node_id == source_node_id {
            return Err(AppError::BadRequest(
                "此连接会形成循环，互动游戏图谱必须保持有向无环图".to_owned(),
            ));
        }
        if visited.insert(node_id.clone()) {
            pending.extend(next_nodes.get(&node_id).into_iter().flatten().cloned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::ensure_acyclic_edge;

    #[test]
    fn rejects_a_manual_edge_that_closes_a_cycle() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE game_edges (id TEXT, game_id TEXT, source_node_id TEXT, target_node_id TEXT)")
            .unwrap();
        connection
            .execute("INSERT INTO game_edges VALUES ('ab','game','a','b')", [])
            .unwrap();
        connection
            .execute("INSERT INTO game_edges VALUES ('bc','game','b','c')", [])
            .unwrap();
        assert!(ensure_acyclic_edge(&connection, "game", None, "c", "a").is_err());
        assert!(ensure_acyclic_edge(&connection, "game", None, "a", "c").is_ok());
    }
}
