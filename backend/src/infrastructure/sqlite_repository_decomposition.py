"""ORM persistence for the script decomposition aggregate."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import delete

from ..domain.models import GenerationStatus
from .orm_models import DramaAsset, DramaShot, DramaShotVersion, ShortDrama
from .repository_common import _json_dump, utc_now


def _mapped_asset_id(value: Any, identifiers: dict[str, str]) -> str:
    """Translate a planner asset id into the project-scoped ORM asset id."""

    raw_id = str(value or "")
    return identifiers.get(raw_id, raw_id)


def _remap_prompt_references(
    nodes: Any, identifiers: dict[str, str]
) -> list[dict[str, Any]]:
    """Keep initial rich-prompt references valid after asset ids are scoped."""

    if not isinstance(nodes, list):
        return []
    remapped: list[dict[str, Any]] = []
    for node in nodes:
        if not isinstance(node, dict):
            continue
        copy = dict(node)
        if copy.get("type") == "reference":
            copy["asset_id"] = _mapped_asset_id(copy.get("asset_id"), identifiers)
        remapped.append(copy)
    return remapped


class DramaRepositoryDecompositionMixin:
    """Replace a drama's generated skeleton with one atomic ORM transaction.

    The decomposition worker calls this after the LLM returns episodes, shots,
    and assets. It rebuilds normalized rows and the legacy aggregate snapshot
    together, so a refresh never sees half of a generated project.
    """

    def save_decomposition(self, drama_id: str, episodes: list[dict[str, Any]],
                           shots: list[dict[str, Any]], assets: list[dict[str, Any]]) -> None:
        timestamp = utc_now()
        valid_statuses = {status.value for status in GenerationStatus}
        normalized_episodes: list[dict[str, Any]] = []
        normalized_assets: list[dict[str, Any]] = []
        normalized_shots: list[dict[str, Any]] = []
        asset_identifiers: dict[str, str] = {}

        for episode_index, episode in enumerate(episodes, start=1):
            raw_id = episode.get("id") or str(uuid4())
            normalized_episodes.append({
                "id": f"{drama_id}:episode:{raw_id}:{episode_index}",
                "sort_order": episode_index,
                "title": episode.get("title", episode.get("name", f"第{episode_index}集")),
            })

        for asset_index, asset in enumerate(assets, start=1):
            raw_id = asset.get("id") or str(uuid4())
            status = asset.get("status", GenerationStatus.NOT_GENERATED.value)
            scoped_id = f"{drama_id}:asset:{raw_id}:{asset_index}"
            asset_identifiers[str(raw_id)] = scoped_id
            normalized_assets.append({
                "id": scoped_id,
                "type": asset.get("type", "prop"), "name": asset.get("name", "未命名元素"),
                "prompt": asset.get("prompt", ""), "voice_id": asset.get("voice_id"),
                "image_url": asset.get("image_url"), "content_hash": asset.get("content_hash"),
                "source_type": asset.get("source_type", "generated"),
                "image_history": asset.get("image_history", []), "variants": asset.get("variants", []),
                "metadata": asset.get("metadata", {}),
                "status": status if status in valid_statuses else GenerationStatus.NOT_GENERATED.value,
            })

        for index, shot in enumerate(shots, start=1):
            raw_id = shot.get("id") or str(uuid4())
            try:
                episode_index = max(1, int(shot.get("episode_index", 1)))
            except (TypeError, ValueError):
                episode_index = 1
            episode = normalized_episodes[min(episode_index, len(normalized_episodes)) - 1] if normalized_episodes else {
                "id": f"{drama_id}:episode:1", "sort_order": 1, "title": "第1集",
            }
            prompt_rich = _remap_prompt_references(
                shot.get("prompt_rich"), asset_identifiers
            )
            references = shot.get("reference_asset_ids") or [
                node.get("asset_id") for node in shot.get("prompt_rich", [])
                if isinstance(node, dict) and node.get("type") == "reference" and node.get("asset_id")
            ]
            references = [_mapped_asset_id(asset_id, asset_identifiers) for asset_id in references]
            normalized_shots.append({
                "id": f"{drama_id}:shot:{raw_id}:{index}", "episode_id": episode["id"],
                "episode_name": episode["title"], "episode_sort_order": episode["sort_order"],
                "shot_index": shot.get("shot_index", index), "title": shot.get("title", f"分镜 {index}"),
                "original_text": shot.get("original_text", shot.get("script", "")),
                "duration_seconds": min(15, max(3, int(shot.get("duration_seconds", shot.get("duration", 10)) or 10))),
                "prompt": shot.get("prompt", ""), "prompt_rich": prompt_rich,
                "placeholder_scene_asset_id": _mapped_asset_id(
                    shot.get("placeholder_scene_asset_id"), asset_identifiers
                ) or None,
                "placeholder_placements": shot.get("placeholder_placements", []),
                "structured": shot.get("structured", {}), "quality": shot.get("quality", {}),
                "quality_status": shot.get("quality_status", "未检查"),
                "quality_issues": shot.get("quality_issues", []), "reference_asset_ids": references,
                "prompt_template_id": shot.get("prompt_template_id"),
                "prompt_template_version": shot.get("prompt_template_version", "v1"),
                "status": shot.get("status", GenerationStatus.NOT_GENERATED.value),
                "historical_videos": shot.get("historical_videos", []),
            })

        with self.database.session() as session:
            drama = session.get(ShortDrama, drama_id)
            if drama is None:
                raise KeyError(f"Project not found: {drama_id}")
            session.execute(delete(DramaShotVersion).where(DramaShotVersion.drama_id == drama_id))
            session.execute(delete(DramaAsset).where(DramaAsset.drama_id == drama_id))
            session.execute(delete(DramaShot).where(DramaShot.drama_id == drama_id))
            session.add_all([
                DramaAsset(
                    id=item["id"], drama_id=drama_id, type=item["type"], name=item["name"],
                    prompt=item["prompt"], voice_id=item["voice_id"], image_url=item["image_url"],
                    content_hash=item["content_hash"], source_type=item["source_type"],
                    image_history_json=_json_dump(item["image_history"]),
                    variants_json=_json_dump(item["variants"]), metadata_json=_json_dump(item["metadata"]),
                    status=item["status"], created_at=timestamp, updated_at=timestamp,
                ) for item in normalized_assets
            ])
            session.add_all([
                DramaShot(
                    id=item["id"], drama_id=drama_id, episode_id=item["episode_id"],
                    episode_name=item["episode_name"], episode_sort_order=item["episode_sort_order"],
                    shot_index=item["shot_index"], title=item["title"], original_text=item["original_text"],
                    duration_seconds=item["duration_seconds"],
                    prompt=item["prompt"], prompt_rich_json=_json_dump(item["prompt_rich"]),
                    placeholder_scene_asset_id=item["placeholder_scene_asset_id"],
                    placeholder_placements_json=_json_dump(item["placeholder_placements"]),
                    structured_json=_json_dump(item["structured"]), quality_json=_json_dump(item["quality"]),
                    quality_status=item["quality_status"], quality_issues_json=_json_dump(item["quality_issues"]),
                    reference_asset_ids_json=_json_dump(item["reference_asset_ids"]),
                    prompt_template_id=item["prompt_template_id"], prompt_template_version=item["prompt_template_version"],
                    status=item["status"], historical_videos_json=_json_dump(item["historical_videos"]),
                    created_at=timestamp, updated_at=timestamp,
                ) for item in normalized_shots
            ])
            drama.shots_json = _json_dump(normalized_shots)
            drama.assets_json = _json_dump(normalized_assets)
            drama.updated_at = timestamp
