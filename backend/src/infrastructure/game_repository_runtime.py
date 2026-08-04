"""ORM persistence for interactive-game editing and playback sessions."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import delete, select

from ..domain.models import GenerationStatus
from .orm_models import GameChoiceEvent, GameEdge, GameNode, GameSession
from .repository_common import _json_dump, _json_load, model_to_row, utc_now


class GameRepositoryRuntimeMixin:
    """Manage graph edits and player state without raw SQL statements.

    The editor calls node/edge methods after user edits, while the game
    runtime calls session methods after a player starts or chooses a branch.
    Both paths use the same ORM transaction so graph and path data stay valid.
    """

    def update_node(self, game_id: str, node_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Update editable node fields from the game editor."""

        allowed = {"title", "original_text", "prompt", "video_url", "status", "duration_seconds", "position_x", "position_y"}
        updates = {key: value for key, value in values.items() if key in allowed and value is not None}
        if not updates:
            raise ValueError("No node fields to update")
        with self.database.session() as session:
            node = session.scalars(
                select(GameNode).where(GameNode.id == node_id, GameNode.game_id == game_id)
            ).first()
            if node is None:
                raise KeyError(f"Game node not found: {node_id}")
            for key, value in updates.items():
                setattr(node, key, value)
            node.updated_at = utc_now()
            session.flush()
            return self._node_from_row(node)

    def add_node_video(self, game_id: str, node_id: str, video: dict[str, Any]) -> dict[str, Any]:
        """Append a generated node-video version and make it current."""

        with self.database.session() as session:
            node = session.scalars(
                select(GameNode).where(GameNode.id == node_id, GameNode.game_id == game_id)
            ).first()
            if node is None:
                raise KeyError(f"Game node not found: {node_id}")
            history = _json_load(node.video_history_json, [])
            history.append(video)
            node.video_history_json = _json_dump(history)
            node.video_url = video.get("url")
            node.status = GenerationStatus.SUCCEEDED.value
            node.updated_at = utc_now()
        return video

    def create_edge(self, game_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Create a selectable branch after validating both endpoint nodes."""

        edge = GameEdge(
            id=str(uuid4()),
            game_id=game_id,
            source_node_id=values["source_node_id"],
            target_node_id=values["target_node_id"],
            option_text=values["option_text"],
            sort_order=values.get("sort_order", 1),
            conditions_json="{}",
            created_at=utc_now(),
            updated_at=utc_now(),
        )
        with self.database.session() as session:
            for node_id in (edge.source_node_id, edge.target_node_id):
                exists = session.scalars(
                    select(GameNode).where(GameNode.id == node_id, GameNode.game_id == game_id)
                ).first()
                if exists is None:
                    raise KeyError(f"Game node not found: {node_id}")
            session.add(edge)
            session.flush()
            return self._edge_from_row(edge)

    def update_edge(self, game_id: str, edge_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Update branch text, ordering, or target from the game editor."""

        allowed = {"target_node_id", "option_text", "sort_order"}
        updates = {key: value for key, value in values.items() if key in allowed and value is not None}
        if not updates:
            raise ValueError("No edge fields to update")
        with self.database.session() as session:
            edge = session.scalars(
                select(GameEdge).where(GameEdge.id == edge_id, GameEdge.game_id == game_id)
            ).first()
            if edge is None:
                raise KeyError(f"Game edge not found: {edge_id}")
            if "target_node_id" in updates:
                target = session.scalars(
                    select(GameNode).where(GameNode.id == updates["target_node_id"], GameNode.game_id == game_id)
                ).first()
                if target is None:
                    raise KeyError(f"Game node not found: {updates['target_node_id']}")
            for key, value in updates.items():
                setattr(edge, key, value)
            edge.updated_at = utc_now()
            session.flush()
            return self._edge_from_row(edge)

    def delete_edge(self, game_id: str, edge_id: str) -> None:
        """Delete one branch selected in the game editor."""

        with self.database.session() as session:
            result = session.execute(
                delete(GameEdge).where(GameEdge.id == edge_id, GameEdge.game_id == game_id)
            )
            if result.rowcount == 0:
                raise KeyError(f"Game edge not found: {edge_id}")

    def create_session(self, game_id: str) -> dict[str, Any]:
        """Start runtime playback at the first start node of a game."""

        with self.database.session() as session:
            start = session.scalars(
                select(GameNode)
                .where(GameNode.game_id == game_id, GameNode.node_type == "start")
                .order_by(GameNode.created_at)
                .limit(1)
            ).first()
            if start is None:
                raise KeyError(f"Start node not found for game: {game_id}")
            current = utc_now()
            runtime_session = GameSession(
                id=str(uuid4()), game_id=game_id, current_node_id=start.id,
                status="active", path_json="[]", created_at=current, updated_at=current,
            )
            session.add(runtime_session)
            session.flush()
            session_id = runtime_session.id
        return self.get_session(session_id) or {}

    def get_session(self, session_id: str) -> dict[str, Any] | None:
        """Load current playback node and available choices for the client."""

        with self.database.session() as session:
            runtime_session = session.get(GameSession, session_id)
            if runtime_session is None:
                return None
            node = session.get(GameNode, runtime_session.current_node_id)
            choices = session.scalars(
                select(GameEdge)
                .where(GameEdge.source_node_id == runtime_session.current_node_id)
                .order_by(GameEdge.sort_order, GameEdge.created_at)
            ).all()
        result = model_to_row(runtime_session)
        result["path"] = _json_load(result.pop("path_json"), [])
        result["current_node"] = self._node_from_row(node) if node else None
        result["choices"] = [self._edge_from_row(edge) for edge in choices]
        return result

    def choose_session_edge(self, session_id: str, edge_id: str) -> dict[str, Any]:
        """Record a player choice and advance or complete the playback session."""

        timestamp = utc_now()
        with self.database.session() as session:
            runtime_session = session.get(GameSession, session_id)
            if runtime_session is None:
                raise KeyError(f"Game session not found: {session_id}")
            if runtime_session.status != "active":
                raise ValueError("Game session has already reached an ending")
            edge = session.scalars(
                select(GameEdge).where(
                    GameEdge.id == edge_id,
                    GameEdge.game_id == runtime_session.game_id,
                    GameEdge.source_node_id == runtime_session.current_node_id,
                )
            ).first()
            if edge is None:
                raise ValueError("The selected edge is not available from the current node")
            target = session.get(GameNode, edge.target_node_id)
            if target is None or target.game_id != runtime_session.game_id:
                raise KeyError(f"Game node not found: {edge.target_node_id}")
            path = _json_load(runtime_session.path_json, [])
            path.append(
                {
                    "edge_id": edge_id,
                    "source_node_id": edge.source_node_id,
                    "target_node_id": edge.target_node_id,
                    "option_text": edge.option_text,
                    "selected_at": timestamp,
                }
            )
            runtime_session.current_node_id = edge.target_node_id
            runtime_session.status = "completed" if target.node_type in {"success", "failure"} else "active"
            runtime_session.path_json = _json_dump(path)
            runtime_session.updated_at = timestamp
            session.add(
                GameChoiceEvent(
                    id=str(uuid4()), session_id=session_id, game_id=runtime_session.game_id,
                    source_node_id=edge.source_node_id, edge_id=edge_id,
                    target_node_id=edge.target_node_id, option_text=edge.option_text,
                    selected_at=timestamp,
                )
            )
        return self.get_session(session_id) or {}
