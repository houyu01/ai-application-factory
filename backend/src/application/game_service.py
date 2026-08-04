"""Application service for interactive game graph workflows."""

from __future__ import annotations

from datetime import datetime, timezone
import logging
from typing import Any

from ..domain.models import (
    GameEdgeCreate,
    GameEdgeUpdate,
    GameNodeUpdate,
    GenerationStatus,
    InteractiveGameCreate,
)
from ..infrastructure.interactive_game_repository import InteractiveGameRepository
from ..infrastructure.media_store import media_store
from ..llm_service.interactive_game_planner import InteractiveGamePlanner


logger = logging.getLogger(__name__)


class InteractiveGameService:
    """Coordinate interactive-game creation, decomposition, and runtime edits.

    The game API calls this service after form submission or editor actions; it
    keeps graph persistence, planner execution, and media cleanup out of the
    FastAPI route layer.
    """

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

    def delete_game(self, game_id: str) -> dict[str, Any]:
        """Delete the complete game graph, runtime history, and owned media."""

        game = self.get_game(game_id)
        media_urls = self._collect_media_urls(game)
        self.repository.delete_game(game_id)

        media_deleted = 0
        cleanup_errors: list[str] = []
        for url in media_urls:
            try:
                if media_store.delete_url(url):
                    media_deleted += 1
            except Exception as exc:  # cloud cleanup must not undo DB deletion
                cleanup_errors.append(str(exc))
                logger.warning("Failed to delete game media %s: %s", url, exc)

        result: dict[str, Any] = {
            "status": "deleted",
            "id": game_id,
            "media_deleted": media_deleted,
        }
        if cleanup_errors:
            result["media_cleanup_errors"] = cleanup_errors
        return result

    @staticmethod
    def _collect_media_urls(value: Any) -> set[str]:
        urls: set[str] = set()

        def visit(node: Any) -> None:
            if isinstance(node, dict):
                for key, child in node.items():
                    if key in {"image_url", "video_url", "url"} and isinstance(child, str):
                        if child.strip():
                            urls.add(child.strip())
                    elif isinstance(child, (dict, list)):
                        visit(child)
            elif isinstance(node, list):
                for child in node:
                    visit(child)

        visit(value)
        return urls

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
        active_task = self.repository.get_active_task(
            game_id, "node_video_generation", node_id
        )
        if active_task is not None:
            return {**active_task, "_reused": True}
        task = self.repository.create_task(
            game_id,
            "node_video_generation",
            node_id,
            {"game_id": game_id, "node_id": node_id},
        )
        self.repository.update_node(
            game_id, node_id, {"status": GenerationStatus.GENERATING.value}
        )
        return {
            **self.repository.update_task_status(task["id"], GenerationStatus.GENERATING),
            "_reused": False,
        }

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
            try:
                self.repository.update_node(
                    game_id, node_id, {"status": GenerationStatus.FAILED.value}
                )
            except KeyError:
                logger.warning("Game node disappeared while video task failed: %s", node_id)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def get_task(self, task_id: str) -> dict[str, Any]:
        task = self.repository.get_task(task_id)
        if task is None:
            raise KeyError(f"Game task not found: {task_id}")
        return task

    def resume_task(self, task: dict[str, Any]) -> None:
        """Resume a persisted game task after a process restart."""
        task_type = str(task.get("type") or "")
        game_id = str(task.get("game_id") or "")
        resource_id = str(task.get("resource_id") or "")
        if task_type == "game_graph_decomposition":
            self.decompose_game(task["id"], game_id)
        elif task_type == "node_video_generation":
            self.run_node_video(task["id"], game_id, resource_id)
        else:
            self.repository.update_task_status(
                task["id"], GenerationStatus.FAILED,
                error_message=f"未知的游戏任务类型：{task_type}",
            )

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
