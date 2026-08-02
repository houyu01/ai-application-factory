"""SQLite persistence for interactive full-motion-video games.

Interactive games use a graph model: nodes are video clips and edges are the
choices shown after a clip. The graph is intentionally separate from drama
tables so both products can evolve without coupling their schemas.
"""

from __future__ import annotations

import json
import os
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator
from uuid import uuid4

from ..domain.models import GenerationStatus, InteractiveGameCreate


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _json_load(value: str | None, default: Any) -> Any:
    if not value:
        return default
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return default


def _json_dump(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False)


class InteractiveGameRepository:
    """Repository using the same local SQLite file as the drama repository."""

    def __init__(self, database_path: str | Path | None = None) -> None:
        default_path = (
            Path(__file__).resolve().parents[2] / "data" / "ai_application_factory.db"
        )
        configured_path = database_path or os.getenv("DATABASE_PATH") or default_path
        self.database_path = Path(configured_path)
        if str(self.database_path) != ":memory:":
            self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self._initialize()

    @contextmanager
    def _connect(self) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(str(self.database_path), timeout=30)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        try:
            yield connection
            connection.commit()
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def _initialize(self) -> None:
        with self._connect() as connection:
            connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS interactive_games (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    script TEXT NOT NULL,
                    platform TEXT NOT NULL,
                    style TEXT NOT NULL,
                    success_ending_count INTEGER NOT NULL,
                    failure_ending_count INTEGER NOT NULL,
                    branch_min INTEGER NOT NULL,
                    branch_max INTEGER NOT NULL,
                    node_duration_min INTEGER NOT NULL,
                    node_duration_max INTEGER NOT NULL,
                    language_model TEXT NOT NULL,
                    multimodal_model TEXT NOT NULL,
                    status TEXT NOT NULL,
                    assets_json TEXT NOT NULL DEFAULT '[]',
                    nodes_json TEXT NOT NULL DEFAULT '[]',
                    edges_json TEXT NOT NULL DEFAULT '[]',
                    historical_videos_json TEXT NOT NULL DEFAULT '[]',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS game_assets (
                    id TEXT PRIMARY KEY,
                    game_id TEXT NOT NULL,
                    type TEXT NOT NULL,
                    name TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    image_url TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS game_nodes (
                    id TEXT PRIMARY KEY,
                    game_id TEXT NOT NULL,
                    node_type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    original_text TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    video_url TEXT,
                    duration_seconds INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    position_x INTEGER NOT NULL DEFAULT 0,
                    position_y INTEGER NOT NULL DEFAULT 0,
                    video_history_json TEXT NOT NULL DEFAULT '[]',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS game_edges (
                    id TEXT PRIMARY KEY,
                    game_id TEXT NOT NULL,
                    source_node_id TEXT NOT NULL,
                    target_node_id TEXT NOT NULL,
                    option_text TEXT NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 1,
                    conditions_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE,
                    FOREIGN KEY (source_node_id) REFERENCES game_nodes(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_node_id) REFERENCES game_nodes(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS game_tasks (
                    id TEXT PRIMARY KEY,
                    game_id TEXT NOT NULL,
                    type TEXT NOT NULL,
                    resource_id TEXT,
                    status TEXT NOT NULL,
                    input_snapshot_json TEXT,
                    result_json TEXT,
                    error_message TEXT,
                    created_at TEXT NOT NULL,
                    started_at TEXT,
                    completed_at TEXT,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS game_sessions (
                    id TEXT PRIMARY KEY,
                    game_id TEXT NOT NULL,
                    current_node_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'active',
                    path_json TEXT NOT NULL DEFAULT '[]',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE,
                    FOREIGN KEY (current_node_id) REFERENCES game_nodes(id)
                );

                CREATE TABLE IF NOT EXISTS game_choice_events (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    game_id TEXT NOT NULL,
                    source_node_id TEXT NOT NULL,
                    edge_id TEXT NOT NULL,
                    target_node_id TEXT NOT NULL,
                    option_text TEXT NOT NULL,
                    selected_at TEXT NOT NULL,
                    FOREIGN KEY (session_id) REFERENCES game_sessions(id) ON DELETE CASCADE,
                    FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_game_assets_game_id ON game_assets(game_id);
                CREATE INDEX IF NOT EXISTS idx_game_nodes_game_id ON game_nodes(game_id);
                CREATE INDEX IF NOT EXISTS idx_game_edges_game_id ON game_edges(game_id);
                CREATE INDEX IF NOT EXISTS idx_game_tasks_game_id ON game_tasks(game_id);
                CREATE INDEX IF NOT EXISTS idx_game_sessions_game_id ON game_sessions(game_id);
                CREATE INDEX IF NOT EXISTS idx_game_choice_events_session_id ON game_choice_events(session_id);
                """
            )

    @staticmethod
    def _game_from_row(row: sqlite3.Row) -> dict[str, Any]:
        game = dict(row)
        for column in (
            "assets_json",
            "nodes_json",
            "edges_json",
            "historical_videos_json",
        ):
            if column in game:
                game[column.removesuffix("_json")] = _json_load(game.pop(column), [])
        return game

    @staticmethod
    def _asset_from_row(row: sqlite3.Row) -> dict[str, Any]:
        return dict(row)

    @staticmethod
    def _node_from_row(row: sqlite3.Row) -> dict[str, Any]:
        node = dict(row)
        node["video_history"] = _json_load(node.pop("video_history_json"), [])
        return node

    @staticmethod
    def _edge_from_row(row: sqlite3.Row) -> dict[str, Any]:
        edge = dict(row)
        edge["conditions"] = _json_load(edge.pop("conditions_json"), {})
        return edge

    @staticmethod
    def _task_from_row(row: sqlite3.Row) -> dict[str, Any]:
        task = dict(row)
        task["input_snapshot"] = _json_load(task.pop("input_snapshot_json"), None)
        task["result"] = _json_load(task.pop("result_json"), None)
        return task

    def create_game_with_task(
        self, payload: InteractiveGameCreate
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        game_id = str(uuid4())
        task_id = str(uuid4())
        timestamp = utc_now()
        values = payload.model_dump()
        with self._connect() as connection:
            connection.execute(
                """
                INSERT INTO interactive_games (
                    id, name, script, platform, style, success_ending_count,
                    failure_ending_count, branch_min, branch_max,
                    node_duration_min, node_duration_max, language_model,
                    multimodal_model, status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    game_id,
                    values["name"],
                    values["script"],
                    values["platform"],
                    values["style"],
                    values["success_ending_count"],
                    values["failure_ending_count"],
                    values["branch_min"],
                    values["branch_max"],
                    values["node_duration_min"],
                    values["node_duration_max"],
                    values["language_model"],
                    values["multimodal_model"],
                    GenerationStatus.NOT_GENERATED.value,
                    timestamp,
                    timestamp,
                ),
            )
            connection.execute(
                """
                INSERT INTO game_tasks (
                    id, game_id, type, status, input_snapshot_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                """,
                (
                    task_id,
                    game_id,
                    "game_graph_decomposition",
                    GenerationStatus.NOT_GENERATED.value,
                    _json_dump({"game_id": game_id, **values}),
                    timestamp,
                ),
            )
            game_row = connection.execute(
                "SELECT * FROM interactive_games WHERE id = ?", (game_id,)
            ).fetchone()
            task_row = connection.execute(
                "SELECT * FROM game_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        assert game_row is not None
        assert task_row is not None
        return self._game_from_row(game_row), self._task_from_row(task_row)

    def list_games(self) -> list[dict[str, Any]]:
        with self._connect() as connection:
            rows = connection.execute(
                """
                SELECT g.*, COUNT(DISTINCT n.id) AS node_count,
                       COUNT(DISTINCT a.id) AS asset_count
                FROM interactive_games g
                LEFT JOIN game_nodes n ON n.game_id = g.id
                LEFT JOIN game_assets a ON a.game_id = g.id
                GROUP BY g.id
                ORDER BY g.created_at DESC
                """
            ).fetchall()
        return [self._game_from_row(row) for row in rows]

    def get_game(self, game_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            game_row = connection.execute(
                "SELECT * FROM interactive_games WHERE id = ?", (game_id,)
            ).fetchone()
            if game_row is None:
                return None
            assets = connection.execute(
                "SELECT * FROM game_assets WHERE game_id = ? ORDER BY created_at, id",
                (game_id,),
            ).fetchall()
            nodes = connection.execute(
                "SELECT * FROM game_nodes WHERE game_id = ? ORDER BY position_y, position_x, created_at",
                (game_id,),
            ).fetchall()
            edges = connection.execute(
                "SELECT * FROM game_edges WHERE game_id = ? ORDER BY source_node_id, sort_order, created_at",
                (game_id,),
            ).fetchall()
            tasks = connection.execute(
                "SELECT * FROM game_tasks WHERE game_id = ? ORDER BY created_at",
                (game_id,),
            ).fetchall()
        game = self._game_from_row(game_row)
        game["assets"] = [self._asset_from_row(row) for row in assets]
        game["nodes"] = [self._node_from_row(row) for row in nodes]
        game["edges"] = [self._edge_from_row(row) for row in edges]
        game["tasks"] = [self._task_from_row(row) for row in tasks]
        return game

    def set_game_status(self, game_id: str, status: GenerationStatus) -> None:
        with self._connect() as connection:
            connection.execute(
                "UPDATE interactive_games SET status = ?, updated_at = ? WHERE id = ?",
                (status.value, utc_now(), game_id),
            )

    def save_graph(
        self,
        game_id: str,
        assets: list[dict[str, Any]],
        nodes: list[dict[str, Any]],
        edges: list[dict[str, Any]],
    ) -> None:
        timestamp = utc_now()
        valid_statuses = {status.value for status in GenerationStatus}
        normalized_assets: list[dict[str, Any]] = []
        normalized_nodes: list[dict[str, Any]] = []
        normalized_edges: list[dict[str, Any]] = []
        node_ids: dict[str, str] = {}

        for index, asset in enumerate(assets, start=1):
            raw_id = str(asset.get("id") or uuid4())
            normalized_assets.append(
                {
                    "id": f"{game_id}:asset:{raw_id}:{index}",
                    "type": asset.get("type", "prop"),
                    "name": asset.get("name", "未命名元素"),
                    "prompt": asset.get("prompt", ""),
                    "image_url": asset.get("image_url"),
                    "status": asset.get("status", GenerationStatus.NOT_GENERATED.value),
                }
            )

        for index, node in enumerate(nodes, start=1):
            raw_id = str(node.get("id") or uuid4())
            normalized_id = f"{game_id}:node:{raw_id}:{index}"
            node_ids[raw_id] = normalized_id
            status = node.get("status", GenerationStatus.NOT_GENERATED.value)
            if status not in valid_statuses:
                status = GenerationStatus.NOT_GENERATED.value
            normalized_nodes.append(
                {
                    "id": normalized_id,
                    "node_type": node.get("node_type", "normal"),
                    "title": node.get("title", f"节点 {index}"),
                    "original_text": node.get("original_text", ""),
                    "prompt": node.get("prompt", ""),
                    "video_url": node.get("video_url"),
                    "duration_seconds": int(node.get("duration_seconds", 10)),
                    "status": status,
                    "position_x": int(node.get("position_x", 80 + (index % 4) * 280)),
                    "position_y": int(node.get("position_y", 80 + (index // 4) * 190)),
                    "video_history": node.get("video_history", []),
                }
            )

        for index, edge in enumerate(edges, start=1):
            source = node_ids.get(str(edge.get("source_node_id")), str(edge.get("source_node_id")))
            target = node_ids.get(str(edge.get("target_node_id")), str(edge.get("target_node_id")))
            normalized_edges.append(
                {
                    "id": f"{game_id}:edge:{edge.get('id') or uuid4()}:{index}",
                    "source_node_id": source,
                    "target_node_id": target,
                    "option_text": edge.get("option_text", f"选项 {index}"),
                    "sort_order": int(edge.get("sort_order", index)),
                    "conditions": edge.get("conditions", {}),
                }
            )

        with self._connect() as connection:
            connection.execute("DELETE FROM game_edges WHERE game_id = ?", (game_id,))
            connection.execute("DELETE FROM game_nodes WHERE game_id = ?", (game_id,))
            connection.execute("DELETE FROM game_assets WHERE game_id = ?", (game_id,))
            connection.executemany(
                """
                INSERT INTO game_assets
                    (id, game_id, type, name, prompt, image_url, status, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    (
                        asset["id"], game_id, asset["type"], asset["name"], asset["prompt"],
                        asset["image_url"], asset["status"], timestamp, timestamp,
                    )
                    for asset in normalized_assets
                ],
            )
            connection.executemany(
                """
                INSERT INTO game_nodes (
                    id, game_id, node_type, title, original_text, prompt, video_url,
                    duration_seconds, status, position_x, position_y,
                    video_history_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    (
                        node["id"], game_id, node["node_type"], node["title"],
                        node["original_text"], node["prompt"], node["video_url"],
                        node["duration_seconds"], node["status"], node["position_x"],
                        node["position_y"], _json_dump(node["video_history"]), timestamp, timestamp,
                    )
                    for node in normalized_nodes
                ],
            )
            connection.executemany(
                """
                INSERT INTO game_edges (
                    id, game_id, source_node_id, target_node_id, option_text,
                    sort_order, conditions_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    (
                        edge["id"], game_id, edge["source_node_id"], edge["target_node_id"],
                        edge["option_text"], edge["sort_order"], _json_dump(edge["conditions"]),
                        timestamp, timestamp,
                    )
                    for edge in normalized_edges
                ],
            )
            connection.execute(
                """
                UPDATE interactive_games
                SET assets_json = ?, nodes_json = ?, edges_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    _json_dump(normalized_assets),
                    _json_dump(normalized_nodes),
                    _json_dump(normalized_edges),
                    timestamp,
                    game_id,
                ),
            )

    def create_task(
        self,
        game_id: str,
        task_type: str,
        resource_id: str | None = None,
        input_snapshot: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        task_id = str(uuid4())
        timestamp = utc_now()
        with self._connect() as connection:
            connection.execute(
                """
                INSERT INTO game_tasks
                    (id, game_id, type, resource_id, status, input_snapshot_json, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    task_id, game_id, task_type, resource_id,
                    GenerationStatus.NOT_GENERATED.value,
                    _json_dump(input_snapshot) if input_snapshot is not None else None,
                    timestamp,
                ),
            )
            row = connection.execute(
                "SELECT * FROM game_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        assert row is not None
        return self._task_from_row(row)

    def get_task(self, task_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM game_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        return self._task_from_row(row) if row else None

    def update_task_status(
        self,
        task_id: str,
        status: GenerationStatus,
        *,
        result: dict[str, Any] | None = None,
        error_message: str | None = None,
    ) -> dict[str, Any]:
        with self._connect() as connection:
            existing = connection.execute(
                "SELECT * FROM game_tasks WHERE id = ?", (task_id,)
            ).fetchone()
            if existing is None:
                raise KeyError(f"Game task not found: {task_id}")
            started_at = existing["started_at"]
            completed_at = existing["completed_at"]
            if status is GenerationStatus.GENERATING and started_at is None:
                started_at = utc_now()
            if status in (GenerationStatus.SUCCEEDED, GenerationStatus.FAILED):
                completed_at = utc_now()
            connection.execute(
                """
                UPDATE game_tasks
                SET status = ?, result_json = ?, error_message = ?,
                    started_at = ?, completed_at = ?
                WHERE id = ?
                """,
                (
                    status.value,
                    _json_dump(result) if result is not None else existing["result_json"],
                    error_message,
                    started_at,
                    completed_at,
                    task_id,
                ),
            )
            row = connection.execute(
                "SELECT * FROM game_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        assert row is not None
        return self._task_from_row(row)

    def update_node(self, game_id: str, node_id: str, values: dict[str, Any]) -> dict[str, Any]:
        allowed = {
            "title", "original_text", "prompt", "video_url", "status",
            "duration_seconds", "position_x", "position_y",
        }
        updates = {key: value for key, value in values.items() if key in allowed and value is not None}
        if not updates:
            raise ValueError("No node fields to update")
        with self._connect() as connection:
            existing = connection.execute(
                "SELECT * FROM game_nodes WHERE id = ? AND game_id = ?",
                (node_id, game_id),
            ).fetchone()
            if existing is None:
                raise KeyError(f"Game node not found: {node_id}")
            assignments = ", ".join(f"{key} = ?" for key in updates)
            connection.execute(
                f"UPDATE game_nodes SET {assignments}, updated_at = ? WHERE id = ? AND game_id = ?",
                (*updates.values(), utc_now(), node_id, game_id),
            )
            row = connection.execute(
                "SELECT * FROM game_nodes WHERE id = ?", (node_id,)
            ).fetchone()
        assert row is not None
        return self._node_from_row(row)

    def add_node_video(self, game_id: str, node_id: str, video: dict[str, Any]) -> dict[str, Any]:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT video_history_json FROM game_nodes WHERE id = ? AND game_id = ?",
                (node_id, game_id),
            ).fetchone()
            if row is None:
                raise KeyError(f"Game node not found: {node_id}")
            history = _json_load(row["video_history_json"], [])
            history.append(video)
            timestamp = utc_now()
            connection.execute(
                """
                UPDATE game_nodes
                SET video_history_json = ?, video_url = ?, status = ?, updated_at = ?
                WHERE id = ? AND game_id = ?
                """,
                (
                    _json_dump(history), video.get("url"), GenerationStatus.SUCCEEDED.value,
                    timestamp, node_id, game_id,
                ),
            )
        return video

    def create_edge(self, game_id: str, values: dict[str, Any]) -> dict[str, Any]:
        edge_id = str(uuid4())
        timestamp = utc_now()
        with self._connect() as connection:
            for node_id in (values["source_node_id"], values["target_node_id"]):
                exists = connection.execute(
                    "SELECT 1 FROM game_nodes WHERE id = ? AND game_id = ?",
                    (node_id, game_id),
                ).fetchone()
                if exists is None:
                    raise KeyError(f"Game node not found: {node_id}")
            connection.execute(
                """
                INSERT INTO game_edges
                    (id, game_id, source_node_id, target_node_id, option_text,
                     sort_order, conditions_json, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, '{}', ?, ?)
                """,
                (
                    edge_id, game_id, values["source_node_id"], values["target_node_id"],
                    values["option_text"], values.get("sort_order", 1), timestamp, timestamp,
                ),
            )
            row = connection.execute(
                "SELECT * FROM game_edges WHERE id = ?", (edge_id,)
            ).fetchone()
        assert row is not None
        return self._edge_from_row(row)

    def update_edge(self, game_id: str, edge_id: str, values: dict[str, Any]) -> dict[str, Any]:
        updates = {
            key: value for key, value in values.items()
            if key in {"target_node_id", "option_text", "sort_order"} and value is not None
        }
        if not updates:
            raise ValueError("No edge fields to update")
        with self._connect() as connection:
            existing = connection.execute(
                "SELECT * FROM game_edges WHERE id = ? AND game_id = ?",
                (edge_id, game_id),
            ).fetchone()
            if existing is None:
                raise KeyError(f"Game edge not found: {edge_id}")
            if "target_node_id" in updates:
                exists = connection.execute(
                    "SELECT 1 FROM game_nodes WHERE id = ? AND game_id = ?",
                    (updates["target_node_id"], game_id),
                ).fetchone()
                if exists is None:
                    raise KeyError(f"Game node not found: {updates['target_node_id']}")
            assignments = ", ".join(f"{key} = ?" for key in updates)
            connection.execute(
                f"UPDATE game_edges SET {assignments}, updated_at = ? WHERE id = ? AND game_id = ?",
                (*updates.values(), utc_now(), edge_id, game_id),
            )
            row = connection.execute(
                "SELECT * FROM game_edges WHERE id = ?", (edge_id,)
            ).fetchone()
        assert row is not None
        return self._edge_from_row(row)

    def delete_edge(self, game_id: str, edge_id: str) -> None:
        with self._connect() as connection:
            cursor = connection.execute(
                "DELETE FROM game_edges WHERE id = ? AND game_id = ?",
                (edge_id, game_id),
            )
            if cursor.rowcount == 0:
                raise KeyError(f"Game edge not found: {edge_id}")

    def create_session(self, game_id: str) -> dict[str, Any]:
        session_id = str(uuid4())
        timestamp = utc_now()
        with self._connect() as connection:
            start = connection.execute(
                """
                SELECT id FROM game_nodes
                WHERE game_id = ? AND node_type = 'start'
                ORDER BY created_at LIMIT 1
                """,
                (game_id,),
            ).fetchone()
            if start is None:
                raise KeyError(f"Start node not found for game: {game_id}")
            connection.execute(
                """
                INSERT INTO game_sessions
                    (id, game_id, current_node_id, status, path_json, created_at, updated_at)
                VALUES (?, ?, ?, 'active', '[]', ?, ?)
                """,
                (session_id, game_id, start["id"], timestamp, timestamp),
            )
        return self.get_session(session_id) or {}

    def get_session(self, session_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            session = connection.execute(
                "SELECT * FROM game_sessions WHERE id = ?", (session_id,)
            ).fetchone()
            if session is None:
                return None
            node = connection.execute(
                "SELECT * FROM game_nodes WHERE id = ?", (session["current_node_id"],)
            ).fetchone()
            choices = connection.execute(
                """
                SELECT * FROM game_edges
                WHERE source_node_id = ?
                ORDER BY sort_order, created_at
                """,
                (session["current_node_id"],),
            ).fetchall()
        result = dict(session)
        result["path"] = _json_load(result.pop("path_json"), [])
        result["current_node"] = self._node_from_row(node) if node else None
        result["choices"] = [self._edge_from_row(edge) for edge in choices]
        return result

    def choose_session_edge(self, session_id: str, edge_id: str) -> dict[str, Any]:
        timestamp = utc_now()
        with self._connect() as connection:
            session = connection.execute(
                "SELECT * FROM game_sessions WHERE id = ?", (session_id,)
            ).fetchone()
            if session is None:
                raise KeyError(f"Game session not found: {session_id}")
            if session["status"] != "active":
                raise ValueError("Game session has already reached an ending")
            edge = connection.execute(
                """
                SELECT * FROM game_edges
                WHERE id = ? AND game_id = ? AND source_node_id = ?
                """,
                (edge_id, session["game_id"], session["current_node_id"]),
            ).fetchone()
            if edge is None:
                raise ValueError("The selected edge is not available from the current node")
            target = connection.execute(
                "SELECT node_type FROM game_nodes WHERE id = ? AND game_id = ?",
                (edge["target_node_id"], session["game_id"]),
            ).fetchone()
            if target is None:
                raise KeyError(f"Game node not found: {edge['target_node_id']}")
            path = _json_load(session["path_json"], [])
            path.append(
                {
                    "edge_id": edge_id,
                    "source_node_id": edge["source_node_id"],
                    "target_node_id": edge["target_node_id"],
                    "option_text": edge["option_text"],
                    "selected_at": timestamp,
                }
            )
            next_status = "completed" if target["node_type"] in {"success", "failure"} else "active"
            connection.execute(
                """
                UPDATE game_sessions
                SET current_node_id = ?, status = ?, path_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (edge["target_node_id"], next_status, _json_dump(path), timestamp, session_id),
            )
            connection.execute(
                """
                INSERT INTO game_choice_events
                    (id, session_id, game_id, source_node_id, edge_id,
                     target_node_id, option_text, selected_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    str(uuid4()), session_id, session["game_id"], edge["source_node_id"],
                    edge_id, edge["target_node_id"], edge["option_text"], timestamp,
                ),
            )
        return self.get_session(session_id) or {}
