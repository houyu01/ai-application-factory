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

    @staticmethod
    def _drama_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        drama = model_to_row(row)
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
    def _shot_version_from_row(row: sqlite3.Row | Any) -> dict[str, Any]:
        item = model_to_row(row)
        item["prompt_rich"] = _json_load(item.pop("prompt_rich_json"), [])
        item["structured"] = _json_load(item.pop("structured_json"), {})
        item["quality"] = _json_load(item.pop("quality_json"), {})
        return item
