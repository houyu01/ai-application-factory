"""ORM persistence for interactive-game projects and their graph."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import delete, desc, select

from ..domain.models import GenerationStatus, InteractiveGameCreate
from .orm_models import (
    GameAsset,
    GameChoiceEvent,
    GameEdge,
    GameNode,
    GameSession,
    GameTask,
    InteractiveGame,
)
from .repository_common import _json_dump, utc_now


class GameRepositoryGraphMixin:
    """Persist the game project and replace its editor graph atomically.

    The gateway calls this slice when the user creates, opens, deletes, or
    saves an interactive-game project. ORM transactions keep the graph rows
    and the compatibility JSON snapshots synchronized without raw SQL.
    """

    def create_game_with_task(
        self, payload: InteractiveGameCreate
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        """Create a game shell and durable decomposition task before planning."""

        values = payload.model_dump()
        timestamp = utc_now()
        game = InteractiveGame(
            id=str(uuid4()),
            name=values["name"],
            script=values["script"],
            platform=values["platform"],
            style=values["style"],
            success_ending_count=values["success_ending_count"],
            failure_ending_count=values["failure_ending_count"],
            branch_min=values["branch_min"],
            branch_max=values["branch_max"],
            node_duration_min=values["node_duration_min"],
            node_duration_max=values["node_duration_max"],
            language_model=values["language_model"],
            multimodal_model=values["multimodal_model"],
            video_model=values.get("video_model", "doubao-seedance-2.0"),
            status=GenerationStatus.NOT_GENERATED.value,
            assets_json="[]",
            nodes_json="[]",
            edges_json="[]",
            historical_videos_json="[]",
            created_at=timestamp,
            updated_at=timestamp,
        )
        task = GameTask(
            id=str(uuid4()),
            game_id=game.id,
            type="game_graph_decomposition",
            resource_id=None,
            status=GenerationStatus.NOT_GENERATED.value,
            input_snapshot_json=_json_dump({"game_id": game.id, **values}),
            created_at=timestamp,
            progress=0,
            stage="",
            poll_attempts=0,
        )
        with self.database.session() as session:
            # Match the drama creation path: persist the FK owner before its
            # durable task so legacy SQLite schemas cannot reject the task.
            session.add(game)
            session.flush()
            session.add(task)
            session.flush()
            return self._game_from_row(game), self._task_from_row(task)

    def list_games(self) -> list[dict[str, Any]]:
        """Load game cards and aggregate normalized asset/node counts."""

        with self.database.session() as session:
            games = session.scalars(select(InteractiveGame).order_by(desc(InteractiveGame.created_at))).all()
            nodes = session.scalars(select(GameNode)).all()
            assets = session.scalars(select(GameAsset)).all()
        node_counts: dict[str, int] = {}
        asset_counts: dict[str, int] = {}
        for node in nodes:
            node_counts[node.game_id] = node_counts.get(node.game_id, 0) + 1
        for asset in assets:
            asset_counts[asset.game_id] = asset_counts.get(asset.game_id, 0) + 1
        result = []
        for game in games:
            item = self._game_from_row(game)
            item["node_count"] = node_counts.get(game.id, 0)
            item["asset_count"] = asset_counts.get(game.id, 0)
            result.append(item)
        return result

    def get_game(self, game_id: str) -> dict[str, Any] | None:
        """Load one complete game graph for the editor and runtime preview."""

        with self.database.session() as session:
            game = session.get(InteractiveGame, game_id)
            if game is None:
                return None
            assets = session.scalars(
                select(GameAsset).where(GameAsset.game_id == game_id).order_by(GameAsset.created_at, GameAsset.id)
            ).all()
            nodes = session.scalars(
                select(GameNode)
                .where(GameNode.game_id == game_id)
                .order_by(GameNode.position_y, GameNode.position_x, GameNode.created_at)
            ).all()
            edges = session.scalars(
                select(GameEdge)
                .where(GameEdge.game_id == game_id)
                .order_by(GameEdge.source_node_id, GameEdge.sort_order, GameEdge.created_at)
            ).all()
            tasks = session.scalars(
                select(GameTask).where(GameTask.game_id == game_id).order_by(GameTask.created_at)
            ).all()
        result = self._game_from_row(game)
        result["assets"] = [self._asset_from_row(row) for row in assets]
        result["nodes"] = [self._node_from_row(row) for row in nodes]
        result["edges"] = [self._edge_from_row(row) for row in edges]
        result["tasks"] = [self._task_from_row(row) for row in tasks]
        return result

    def delete_game(self, game_id: str) -> None:
        """Delete a game and every graph, task, session, and choice child row."""

        with self.database.session() as session:
            if session.get(InteractiveGame, game_id) is None:
                raise KeyError(f"Interactive game not found: {game_id}")
            for model in (GameChoiceEvent, GameSession, GameEdge, GameNode, GameTask, GameAsset):
                session.execute(delete(model).where(getattr(model, "game_id") == game_id))
            session.execute(delete(InteractiveGame).where(InteractiveGame.id == game_id))

    def update_model_selection(self, game_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Update the model names selected for one interactive game."""

        allowed = {"language_model", "multimodal_model", "video_model"}
        updates = {key: str(value).strip() for key, value in values.items() if key in allowed and str(value).strip()}
        if not updates:
            raise ValueError("No model fields to update")
        with self.database.session() as session:
            game = session.get(InteractiveGame, game_id)
            if game is None:
                raise KeyError(f"Interactive game not found: {game_id}")
            for key, value in updates.items():
                setattr(game, key, value)
            game.updated_at = utc_now()
            session.flush()
            return self._game_from_row(game)

    def set_game_status(self, game_id: str, status: GenerationStatus) -> None:
        """Persist graph-generation status for durable refresh/restart recovery."""

        with self.database.session() as session:
            game = session.get(InteractiveGame, game_id)
            if game is not None:
                game.status = status.value
                game.updated_at = utc_now()

    def save_graph(
        self,
        game_id: str,
        assets: list[dict[str, Any]],
        nodes: list[dict[str, Any]],
        edges: list[dict[str, Any]],
    ) -> None:
        """Replace graph rows and snapshots after planner output is accepted."""

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

        with self.database.session() as session:
            game = session.get(InteractiveGame, game_id)
            if game is None:
                raise KeyError(f"Interactive game not found: {game_id}")
            for model in (GameEdge, GameNode, GameAsset):
                session.execute(delete(model).where(getattr(model, "game_id") == game_id))
            session.add_all(
                [
                    GameAsset(
                        id=item["id"], game_id=game_id, type=item["type"], name=item["name"],
                        prompt=item["prompt"], image_url=item["image_url"], status=item["status"],
                        created_at=timestamp, updated_at=timestamp,
                    )
                    for item in normalized_assets
                ]
            )
            session.add_all(
                [
                    GameNode(
                        id=item["id"], game_id=game_id, node_type=item["node_type"], title=item["title"],
                        original_text=item["original_text"], prompt=item["prompt"], video_url=item["video_url"],
                        duration_seconds=item["duration_seconds"], status=item["status"],
                        position_x=item["position_x"], position_y=item["position_y"],
                        video_history_json=_json_dump(item["video_history"]), created_at=timestamp,
                        updated_at=timestamp,
                    )
                    for item in normalized_nodes
                ]
            )
            session.add_all(
                [
                    GameEdge(
                        id=item["id"], game_id=game_id, source_node_id=item["source_node_id"],
                        target_node_id=item["target_node_id"], option_text=item["option_text"],
                        sort_order=item["sort_order"], conditions_json=_json_dump(item["conditions"]),
                        created_at=timestamp, updated_at=timestamp,
                    )
                    for item in normalized_edges
                ]
            )
            game.assets_json = _json_dump(normalized_assets)
            game.nodes_json = _json_dump(normalized_nodes)
            game.edges_json = _json_dump(normalized_edges)
            game.updated_at = timestamp
