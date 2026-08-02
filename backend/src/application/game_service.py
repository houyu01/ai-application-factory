"""Application service for interactive game graph workflows."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from ..domain.models import (
    GameEdgeCreate,
    GameEdgeUpdate,
    GameNodeUpdate,
    GenerationStatus,
    InteractiveGameCreate,
)
from ..infrastructure.interactive_game_repository import InteractiveGameRepository
from ..llm_service.interactive_game_planner import InteractiveGamePlanner


class InteractiveGameService:
    def __init__(
        self,
        repository: InteractiveGameRepository | None = None,
        planner: InteractiveGamePlanner | None = None,
    ) -> None:
        self.repository = repository or InteractiveGameRepository()
        self.planner = planner or InteractiveGamePlanner()

    def create_game(self, payload: InteractiveGameCreate) -> dict[str, Any]:
        game, task = self.repository.create_game_with_task(payload)
        self.repository.set_game_status(game["id"], GenerationStatus.GENERATING)
        task = self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)
        game["status"] = GenerationStatus.GENERATING.value
        game["task_id"] = task["id"]
        game["task"] = task
        return game

    def list_games(self) -> list[dict[str, Any]]:
        return self.repository.list_games()

    def get_game(self, game_id: str) -> dict[str, Any]:
        game = self.repository.get_game(game_id)
        if game is None:
            raise KeyError(f"Interactive game not found: {game_id}")
        return game

    def decompose_game(self, task_id: str, game_id: str) -> None:
        try:
            game = self.get_game(game_id)
            graph = self.planner.plan(game)
            self.repository.save_graph(
                game_id,
                graph.get("assets", []),
                graph.get("nodes", []),
                graph.get("edges", []),
            )
            self.repository.set_game_status(game_id, GenerationStatus.SUCCEEDED)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "assets": graph.get("assets", []),
                    "nodes": graph.get("nodes", []),
                    "edges": graph.get("edges", []),
                },
            )
        except Exception as exc:
            self.repository.set_game_status(game_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def enqueue_node_video(self, game_id: str, node_id: str) -> dict[str, Any]:
        game = self.repository.get_game(game_id)
        if game is None:
            raise KeyError(f"Interactive game not found: {game_id}")
        if not any(node["id"] == node_id for node in game["nodes"]):
            raise KeyError(f"Game node not found: {node_id}")
        task = self.repository.create_task(
            game_id,
            "node_video_generation",
            node_id,
            {"game_id": game_id, "node_id": node_id},
        )
        return self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)

    def run_node_video(
        self,
        task_id: str,
        game_id: str,
        node_id: str,
        video_url: str | None = None,
    ) -> None:
        try:
            video = {
                "id": task_id,
                "url": video_url,
                "generated_at": datetime.now(timezone.utc).isoformat(),
                "task_id": task_id,
            }
            self.repository.add_node_video(game_id, node_id, video)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={"node_id": node_id, **video},
            )
        except Exception as exc:
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def get_task(self, task_id: str) -> dict[str, Any]:
        task = self.repository.get_task(task_id)
        if task is None:
            raise KeyError(f"Game task not found: {task_id}")
        return task

    def update_node(
        self, game_id: str, node_id: str, payload: GameNodeUpdate
    ) -> dict[str, Any]:
        self.get_game(game_id)
        return self.repository.update_node(game_id, node_id, payload.model_dump(exclude_none=True))

    def create_edge(
        self, game_id: str, payload: GameEdgeCreate
    ) -> dict[str, Any]:
        self.get_game(game_id)
        return self.repository.create_edge(game_id, payload.model_dump())

    def update_edge(
        self, game_id: str, edge_id: str, payload: GameEdgeUpdate
    ) -> dict[str, Any]:
        self.get_game(game_id)
        return self.repository.update_edge(
            game_id, edge_id, payload.model_dump(exclude_none=True)
        )

    def delete_edge(self, game_id: str, edge_id: str) -> None:
        self.get_game(game_id)
        self.repository.delete_edge(game_id, edge_id)

    def create_session(self, game_id: str) -> dict[str, Any]:
        game = self.get_game(game_id)
        if not game.get("nodes"):
            raise ValueError("Game graph is not ready")
        return self.repository.create_session(game_id)

    def get_session(self, game_id: str, session_id: str) -> dict[str, Any]:
        self.get_game(game_id)
        session = self.repository.get_session(session_id)
        if session is None or session["game_id"] != game_id:
            raise KeyError(f"Game session not found: {session_id}")
        return session

    def choose_session_edge(
        self, game_id: str, session_id: str, edge_id: str
    ) -> dict[str, Any]:
        self.get_game(game_id)
        session = self.repository.get_session(session_id)
        if session is None or session["game_id"] != game_id:
            raise KeyError(f"Game session not found: {session_id}")
        return self.repository.choose_session_edge(session_id, edge_id)


game_service = InteractiveGameService()
