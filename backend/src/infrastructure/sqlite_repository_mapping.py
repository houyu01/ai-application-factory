"""Persistence behavior extracted from the public repository facade."""

from __future__ import annotations

import json
import os
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Iterator
from uuid import uuid4

from ..domain.models import GenerationStatus, InteractiveGameCreate, ProjectCreate
from ..domain.voice_presets import DEFAULT_VOICE_PRESETS
from .repository_common import JSON_FIELDS, _json_dump, _json_load, _parse_datetime, model_to_row, utc_now


class DramaRepositoryMappingMixin:
    """Owns the DramaRepositoryMapping persistence slice."""

    DETAIL_EXPANDED_PREVIEW_LIMIT = 3_200

    @staticmethod
    def _drama_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        drama = model_to_row(row)
        drama.pop("expanded_script", None)
        for column, output_key in JSON_FIELDS.items():
            if column in drama:
                default = {} if output_key in {"asset_public_prompts", "shot_constraints"} else []
                drama[output_key] = _json_load(drama.pop(column), default)
        drama.setdefault("episodes", [])
        drama.setdefault("shots", [])
        drama.setdefault("assets", [])
        drama.setdefault("historical_videos", [])
        return drama

    @staticmethod
    def _asset_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        asset = model_to_row(row)
        image_history = _json_load(asset.pop("image_history_json", None), [])
        variants = _json_load(asset.pop("variants_json", None), [])
        metadata = _json_load(asset.pop("metadata_json", None), {})
        asset["image_history"] = image_history if isinstance(image_history, list) else []
        asset["variants"] = variants if isinstance(variants, list) else []
        asset["metadata"] = metadata if isinstance(metadata, dict) else {}
        return asset

    @staticmethod
    def _shot_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        shot = model_to_row(row)
        shot["duration_seconds"] = min(15, max(3, int(shot.get("duration_seconds") or 10)))
        shot["duration"] = shot["duration_seconds"]
        shot["historical_videos"] = _json_load(shot.pop("historical_videos_json"), [])
        prompt_rich_value = _json_load(shot.pop("prompt_rich_json", None), [])
        shot["prompt_rich"] = prompt_rich_value if isinstance(prompt_rich_value, list) else []
        placements = _json_load(shot.pop("placeholder_placements_json", None), [])
        shot["placeholder_placements"] = placements if isinstance(placements, list) else []
        structured = _json_load(shot.pop("structured_json", None), {})
        shot["structured"] = structured if isinstance(structured, dict) else {}
        shot["first_last_frames"] = shot["structured"].get("first_last_frames", {})
        quality = _json_load(shot.pop("quality_json", None), {})
        shot["quality"] = quality if isinstance(quality, dict) else {}
        quality_issues = _json_load(shot.pop("quality_issues_json", None), [])
        shot["quality_issues"] = quality_issues if isinstance(quality_issues, list) else []
        references = _json_load(shot.pop("reference_asset_ids_json", None), [])
        shot["reference_asset_ids"] = references if isinstance(references, list) else []
        return shot

    @staticmethod
    def _aggregate_episodes(shots: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Build the public episode view from the episode fields on each shot."""

        grouped: dict[str, dict[str, Any]] = {}
        for shot in shots:
            episode_id = str(shot.get("episode_id") or "episode:1")
            raw_sort_order = shot.get("episode_sort_order", 1)
            try:
                sort_order = max(1, int(raw_sort_order))
            except (TypeError, ValueError):
                sort_order = 1
            episode = grouped.setdefault(
                episode_id,
                {
                    "id": episode_id,
                    "sort_order": sort_order,
                    "title": shot.get("episode_name") or f"第{sort_order}集",
                    "shot_count": 0,
                },
            )
            episode["shot_count"] += 1

        return sorted(
            grouped.values(),
            key=lambda episode: (episode["sort_order"], episode["id"]),
        )

    @staticmethod
    def _task_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        task = model_to_row(row)
        # The persistence schema calls this foreign key ``drama_id`` while
        # the public API uses the provider-neutral ``project_id`` name.
        task["project_id"] = task["drama_id"]
        output_value = task.pop("output_result_json", None)
        legacy_result_value = task.pop("result_json", None)
        result_value = output_value or legacy_result_value
        input_value = task.pop("input_snapshot_json", None)
        task["input_snapshot"] = _json_load(input_value, None)
        task["result"] = _json_load(result_value, None)
        return task

    @staticmethod
    def _detail_task_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        """Build the bounded task projection used by the project detail page.

        The detail editor renders task progress and only reads ``shot_id``
        and ``expanded_script_preview`` from task input. Full provider results
        remain available to worker/repository flows but must not be shipped in
        every initial project response.
        """
        task = DramaRepositoryMappingMixin._task_from_row(row)
        input_snapshot = task.get("input_snapshot")
        if isinstance(input_snapshot, dict):
            detail_input = {
                key: input_snapshot[key]
                for key in ("shot_id", "expanded_script_preview")
                if key in input_snapshot
            }
            preview = detail_input.get("expanded_script_preview")
            if isinstance(preview, str):
                detail_input["expanded_script_preview"] = (
                    DramaRepositoryMappingMixin._detail_expanded_preview(preview)
                )
            task["input_snapshot"] = detail_input
        else:
            task["input_snapshot"] = None
        # Bootstrap tasks are the sole exception: the project editor needs the
        # two persisted screenplay lengths to render completion state after a
        # refresh.  Do not expose the potentially huge episodes/assets payload.
        if task.get("type") == "script_decomposition" and isinstance(task.get("result"), dict):
            task["result"] = {
                key: task["result"].get(key)
                for key in ("original_script_length", "expanded_script_length")
                if key in task["result"]
            }
        else:
            task["result"] = None
        return task

    @staticmethod
    def _detail_expanded_preview(value: str) -> str:
        """Keep the task banner useful without returning a whole screenplay."""
        limit = DramaRepositoryMappingMixin.DETAIL_EXPANDED_PREVIEW_LIMIT
        if len(value) <= limit:
            return value
        head_length = 2_400
        tail_length = limit - head_length
        omitted = len(value) - limit
        return f"{value[:head_length]}\n\n…（已省略 {omitted:,} 字）…\n\n{value[-tail_length:]}"

    @staticmethod
    def _shot_version_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        item = model_to_row(row)
        item["prompt_rich"] = _json_load(item.pop("prompt_rich_json"), [])
        item["structured"] = _json_load(item.pop("structured_json"), {})
        item["quality"] = _json_load(item.pop("quality_json"), {})
        return item
