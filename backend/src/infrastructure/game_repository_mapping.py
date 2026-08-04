"""Convert ORM records into the API-compatible interactive-game shapes."""

from __future__ import annotations

from typing import Any

from .repository_common import _json_load, model_to_row


class GameRepositoryMappingMixin:
    """Owns the GameRepositoryMapping persistence slice."""

    @staticmethod
    def _game_from_row(row: Any) -> dict[str, Any]:
        game = model_to_row(row)
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
    def _asset_from_row(row: Any) -> dict[str, Any]:
        return model_to_row(row)

    @staticmethod
    def _node_from_row(row: Any) -> dict[str, Any]:
        node = model_to_row(row)
        node["video_history"] = _json_load(node.pop("video_history_json"), [])
        return node

    @staticmethod
    def _edge_from_row(row: Any) -> dict[str, Any]:
        edge = model_to_row(row)
        edge["conditions"] = _json_load(edge.pop("conditions_json"), {})
        return edge

    @staticmethod
    def _task_from_row(row: Any) -> dict[str, Any]:
        task = model_to_row(row)
        task["input_snapshot"] = _json_load(task.pop("input_snapshot_json"), None)
        task["result"] = _json_load(task.pop("result_json"), None)
        return task
