"""ORM persistence for shot editing and historical video versions."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import delete, desc, select

from ..domain.models import GenerationStatus
from .orm_models import DramaAsset, DramaShot, DramaShotVersion, GenerationTask, ShortDrama
from .repository_common import _json_dump, _json_load, utc_now


class DramaRepositoryShotMixin:
    """Manage shot prompts, structured fields, quality data, and video history.

    The shot editor calls these methods for every field save, while video
    workers create and update version rows so each generation remains visible.
    """

    def get_shot(self, drama_id: str, shot_id: str) -> dict[str, Any] | None:
        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            if shot is None or shot.drama_id != drama_id:
                return None
            return self._shot_from_row(shot)

    def list_shots(self, drama_id: str) -> list[dict[str, Any]]:
        """Return only this drama's shots and versions for partial refreshes."""
        with self.database.session() as session:
            shots = session.scalars(
                select(DramaShot)
                .where(DramaShot.drama_id == drama_id)
                .order_by(
                    DramaShot.episode_sort_order,
                    DramaShot.episode_name,
                    DramaShot.shot_index,
                    DramaShot.created_at,
                )
            ).all()
            versions = session.scalars(
                select(DramaShotVersion)
                .where(DramaShotVersion.drama_id == drama_id)
                .order_by(DramaShotVersion.shot_id, desc(DramaShotVersion.version_no))
            ).all()
        versions_by_shot: dict[str, list[dict[str, Any]]] = {}
        for version in versions:
            versions_by_shot.setdefault(version.shot_id, []).append(self._shot_version_from_row(version))
        result = [self._shot_from_row(shot) for shot in shots]
        for shot in result:
            shot["versions"] = versions_by_shot.get(str(shot["id"]), [])
        return result

    def _sync_shot_snapshot(self, session: Any, drama: ShortDrama) -> None:
        """Keep the compatibility snapshot aligned with normalized shot rows."""
        shots = session.scalars(select(DramaShot).where(
            DramaShot.drama_id == drama.id
        ).order_by(
            DramaShot.episode_sort_order, DramaShot.episode_name,
            DramaShot.shot_index, DramaShot.created_at,
        )).all()
        drama.shots_json = _json_dump([self._shot_from_row(shot) for shot in shots])
        drama.updated_at = utc_now()

    def create_shot_after(self, drama_id: str, after_shot_id: str, *,
                          title: str = "未命名分镜", original_text: str = "",
                          prompt: str = "", prompt_rich: list[dict[str, Any]] | None = None) -> dict[str, Any]:
        """Insert an empty shot below the selected shot and renumber its episode."""
        timestamp = utc_now()
        with self.database.session() as session:
            current = session.get(DramaShot, after_shot_id)
            drama = session.get(ShortDrama, drama_id)
            if current is None or current.drama_id != drama_id or drama is None:
                raise KeyError(f"Shot not found: {after_shot_id}")
            siblings = session.scalars(select(DramaShot).where(
                DramaShot.drama_id == drama_id,
                DramaShot.episode_id == current.episode_id,
                DramaShot.shot_index > current.shot_index,
            )).all()
            for sibling in siblings:
                sibling.shot_index += 1
            shot = DramaShot(
                id=f"{drama_id}:shot:{uuid4()}", drama_id=drama_id,
                episode_id=current.episode_id, episode_name=current.episode_name,
                episode_sort_order=current.episode_sort_order,
                shot_index=current.shot_index + 1, title=title,
                original_text=original_text, duration_seconds=10, prompt=prompt,
                prompt_rich_json=_json_dump(prompt_rich or []),
                placeholder_placements_json="[]", structured_json="{}",
                quality_json="{}", quality_status="未检查", quality_issues_json="[]",
                reference_asset_ids_json="[]", prompt_template_version="v1",
                status=GenerationStatus.NOT_GENERATED.value,
                historical_videos_json="[]", created_at=timestamp, updated_at=timestamp,
            )
            session.add(shot)
            session.flush()
            self._sync_shot_snapshot(session, drama)
            return self._shot_from_row(shot)

    def delete_shot(self, drama_id: str, shot_id: str) -> dict[str, Any]:
        """Stop and remove a shot, its versions, tasks, and shot-owned placeholders."""
        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            drama = session.get(ShortDrama, drama_id)
            if shot is None or shot.drama_id != drama_id or drama is None:
                raise KeyError(f"Shot not found: {shot_id}")
            media_urls: set[str] = set()
            history = _json_load(shot.historical_videos_json, [])
            if isinstance(history, list):
                media_urls.update(str(item.get("url")) for item in history if isinstance(item, dict) and item.get("url"))
            versions = session.scalars(select(DramaShotVersion).where(
                DramaShotVersion.drama_id == drama_id, DramaShotVersion.shot_id == shot_id
            )).all()
            media_urls.update(str(version.video_url) for version in versions if version.video_url)
            placeholders = []
            assets = session.scalars(select(DramaAsset).where(DramaAsset.drama_id == drama_id)).all()
            for asset in assets:
                metadata = _json_load(asset.metadata_json, {})
                if asset.type != "placeholder" or not isinstance(metadata, dict) or str(metadata.get("shot_id")) != shot_id:
                    continue
                placeholders.append(asset)
                if asset.image_url:
                    media_urls.add(asset.image_url)
                asset_history = _json_load(asset.image_history_json, [])
                if isinstance(asset_history, list):
                    media_urls.update(str(item.get("url")) for item in asset_history if isinstance(item, dict) and item.get("url"))
            cancelled_task_ids: list[str] = []
            provider_task_ids: list[str] = []
            tasks = session.scalars(select(GenerationTask).where(
                GenerationTask.drama_id == drama_id
            )).all()
            for task in tasks:
                snapshot = _json_load(task.input_snapshot_json, {})
                belongs_to_shot = task.resource_id == shot_id or (
                    isinstance(snapshot, dict) and str(snapshot.get("shot_id") or snapshot.get("resource_id")) == shot_id
                )
                if belongs_to_shot and task.status == GenerationStatus.GENERATING.value:
                    cancelled_task_ids.append(task.id)
                    if task.provider_task_id:
                        provider_task_ids.append(task.provider_task_id)
                    task.status = GenerationStatus.FAILED.value
                    task.error_message = "分镜已删除，任务已取消"
                    task.completed_at = utc_now()
                    task.finished_at = task.completed_at
                    task.progress = 100
                    task.next_poll_at = None
                    task.poll_lease_token = None
                    task.poll_lease_until = None
                if belongs_to_shot:
                    session.delete(task)
            for placeholder in placeholders:
                session.delete(placeholder)
            session.execute(delete(DramaShotVersion).where(DramaShotVersion.shot_id == shot_id))
            session.delete(shot)
            session.flush()
            self._sync_shot_snapshot(session, drama)
            return {
                "status": "deleted", "id": shot_id,
                "cancelled_task_ids": cancelled_task_ids,
                "provider_task_ids": provider_task_ids,
                "media_urls": sorted(media_urls),
            }

    def update_shot(self, drama_id: str, shot_id: str, *, title: str | None = None,
                    original_text: str | None = None, prompt: str | None = None,
                    prompt_rich: list[dict[str, Any]] | None = None,
                    duration_seconds: int | None = None,
                    placeholder_scene_asset_id: str | None = None,
                    placeholder_placements: list[dict[str, Any]] | None = None,
                    structured: dict[str, Any] | None = None, quality: dict[str, Any] | None = None,
                    quality_status: str | None = None, quality_issues: list[dict[str, Any]] | None = None,
                    reference_asset_ids: list[str] | None = None, prompt_template_id: str | None = None,
                    prompt_template_version: str | None = None, first_last_frames: dict[str, Any] | None = None,
                    status: GenerationStatus | None = None) -> dict[str, Any]:
        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            if shot is None or shot.drama_id != drama_id:
                raise KeyError(f"Shot not found: {shot_id}")
            if title is not None: shot.title = title
            if original_text is not None: shot.original_text = original_text
            if duration_seconds is not None: shot.duration_seconds = min(15, max(3, int(duration_seconds)))
            if prompt is not None: shot.prompt = prompt
            if prompt_rich is not None: shot.prompt_rich_json = _json_dump(prompt_rich)
            if placeholder_scene_asset_id is not None: shot.placeholder_scene_asset_id = placeholder_scene_asset_id
            if placeholder_placements is not None: shot.placeholder_placements_json = _json_dump(placeholder_placements)
            if structured is not None: shot.structured_json = _json_dump(structured)
            if first_last_frames is not None:
                current_structured = _json_load(shot.structured_json, {})
                current_structured["first_last_frames"] = first_last_frames
                shot.structured_json = _json_dump(current_structured)
            if quality is not None: shot.quality_json = _json_dump(quality)
            if quality_status is not None: shot.quality_status = quality_status
            if quality_issues is not None: shot.quality_issues_json = _json_dump(quality_issues)
            if reference_asset_ids is not None: shot.reference_asset_ids_json = _json_dump(reference_asset_ids)
            if prompt_template_id is not None: shot.prompt_template_id = prompt_template_id
            if prompt_template_version is not None: shot.prompt_template_version = prompt_template_version
            if status is not None: shot.status = status.value
            shot.updated_at = utc_now()
            session.flush()
            return self._shot_from_row(shot)

    def create_shot_version(self, drama_id: str, shot_id: str, *, task_id: str | None = None,
                            prompt: str = "", prompt_rich: list[dict[str, Any]] | None = None,
                            structured: dict[str, Any] | None = None, quality: dict[str, Any] | None = None,
                            status: GenerationStatus = GenerationStatus.GENERATING,
                            provider_task_id: str | None = None) -> dict[str, Any]:
        timestamp = utc_now()
        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            if shot is None or shot.drama_id != drama_id:
                raise KeyError(f"Shot not found: {shot_id}")
            previous = session.scalars(select(DramaShotVersion).where(
                DramaShotVersion.shot_id == shot_id).order_by(desc(DramaShotVersion.version_no)).limit(1)).first()
            version = DramaShotVersion(
                id=str(uuid4()), drama_id=drama_id, shot_id=shot_id, task_id=task_id,
                version_no=(previous.version_no + 1 if previous else 1), status=status.value,
                prompt=prompt, prompt_rich_json=_json_dump(prompt_rich or []),
                structured_json=_json_dump(structured or {}), quality_json=_json_dump(quality or {}),
                provider_task_id=provider_task_id, created_at=timestamp,
            )
            session.add(version)
            session.flush()
            return self._shot_version_from_row(version)

    def list_shot_versions(self, drama_id: str, shot_id: str) -> list[dict[str, Any]]:
        with self.database.session() as session:
            versions = session.scalars(select(DramaShotVersion).where(
                DramaShotVersion.drama_id == drama_id, DramaShotVersion.shot_id == shot_id
            ).order_by(desc(DramaShotVersion.version_no))).all()
            return [self._shot_version_from_row(version) for version in versions]

    def update_shot_version(self, version_id: str, *, status: GenerationStatus | None = None,
                            progress: int | None = None, task_id: str | None = None,
                            provider_task_id: str | None = None, video_url: str | None = None,
                            error_message: str | None = None) -> dict[str, Any]:
        with self.database.session() as session:
            version = session.get(DramaShotVersion, version_id)
            if version is None:
                raise KeyError(f"Shot version not found: {version_id}")
            if status is not None: version.status = status.value
            if progress is not None: version.progress = max(0, min(100, int(progress)))
            if task_id is not None: version.task_id = task_id
            if provider_task_id is not None: version.provider_task_id = provider_task_id
            if video_url is not None: version.video_url = video_url
            if error_message is not None: version.error_message = error_message
            if status in (
                GenerationStatus.SUCCEEDED,
                GenerationStatus.FAILED,
                GenerationStatus.CANCELLED,
            ):
                version.completed_at = utc_now()
                if status is not GenerationStatus.CANCELLED:
                    version.progress = 100
            session.flush()
            return self._shot_version_from_row(version)

    def add_historical_video(self, drama_id: str, shot_id: str, video: dict[str, Any]) -> dict[str, Any]:
        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            drama = session.get(ShortDrama, drama_id)
            if shot is None or shot.drama_id != drama_id or drama is None:
                raise KeyError(f"Shot not found: {shot_id}")
            shot_history = _json_load(shot.historical_videos_json, [])
            drama_history = _json_load(drama.historical_videos_json, [])
            shot_history = shot_history if isinstance(shot_history, list) else []
            drama_history = drama_history if isinstance(drama_history, list) else []
            timestamp = utc_now()
            shot_history.append(video)
            drama_history.append({**video, "shot_id": shot_id})
            shot.historical_videos_json = _json_dump(shot_history)
            shot.status = GenerationStatus.SUCCEEDED.value
            shot.updated_at = timestamp
            drama.historical_videos_json = _json_dump(drama_history)
            drama.updated_at = timestamp
            return video

    def delete_historical_video(
        self, drama_id: str, shot_id: str, video_id: str
    ) -> dict[str, Any]:
        """Delete a video history record, its durable version, and its generation task.

        The video-history delete action calls this after a user removes a past
        success, failure, or in-progress run. Removing each related record keeps
        the per-shot history and durable task queue free from orphaned data.
        """

        with self.database.session() as session:
            shot = session.get(DramaShot, shot_id)
            drama = session.get(ShortDrama, drama_id)
            if shot is None or shot.drama_id != drama_id or drama is None:
                raise KeyError(f"Shot not found: {shot_id}")
            versions = session.scalars(select(DramaShotVersion).where(
                DramaShotVersion.drama_id == drama_id,
                DramaShotVersion.shot_id == shot_id,
            )).all()
            version = next((item for item in versions if video_id in {
                str(item.id), str(item.task_id or "")
            }), None)
            shot_history = _json_load(shot.historical_videos_json, [])
            shot_history = shot_history if isinstance(shot_history, list) else []
            historical = next((item for item in shot_history if isinstance(item, dict) and video_id in {
                str(item.get("id") or ""), str(item.get("task_id") or "")
            }), None)
            if version is None and historical is None:
                raise KeyError(f"Historical video not found: {video_id}")
            task_ids = {str(value) for value in (
                version.task_id if version else None,
                historical.get("task_id") if historical else None,
                historical.get("id") if historical else None,
            ) if value}
            media_urls = {str(value) for value in (
                version.video_url if version else None,
                historical.get("url") if historical else None,
            ) if value}

            def matches_history(item: Any) -> bool:
                if not isinstance(item, dict):
                    return False
                item_ids = {str(item.get("id") or ""), str(item.get("task_id") or "")}
                return bool(item_ids.intersection(task_ids) or str(item.get("url") or "") in media_urls)

            shot.historical_videos_json = _json_dump([
                item for item in shot_history if not matches_history(item)
            ])
            drama_history = _json_load(drama.historical_videos_json, [])
            drama_history = drama_history if isinstance(drama_history, list) else []
            drama.historical_videos_json = _json_dump([
                item for item in drama_history
                if not (isinstance(item, dict) and str(item.get("shot_id") or "") == shot_id and matches_history(item))
            ])
            if version is not None:
                session.delete(version)
            provider_task_ids: list[str] = []
            for task_id in task_ids:
                task = session.get(GenerationTask, task_id)
                if task is None:
                    continue
                if task.provider_task_id:
                    provider_task_ids.append(task.provider_task_id)
                session.delete(task)
            timestamp = utc_now()
            session.flush()
            remaining_versions = session.scalars(select(DramaShotVersion).where(
                DramaShotVersion.drama_id == drama_id,
                DramaShotVersion.shot_id == shot_id,
            )).all()
            remaining_history = _json_load(shot.historical_videos_json, [])
            remaining_statuses = {item.status for item in remaining_versions}
            if GenerationStatus.GENERATING.value in remaining_statuses:
                shot.status = GenerationStatus.GENERATING.value
            elif GenerationStatus.SUCCEEDED.value in remaining_statuses or remaining_history:
                shot.status = GenerationStatus.SUCCEEDED.value
            elif GenerationStatus.FAILED.value in remaining_statuses:
                shot.status = GenerationStatus.FAILED.value
            else:
                shot.status = GenerationStatus.NOT_GENERATED.value
            shot.updated_at = timestamp
            drama.updated_at = timestamp
            self._sync_shot_snapshot(session, drama)
            return {
                "id": video_id,
                "task_ids": sorted(task_ids),
                "provider_task_ids": provider_task_ids,
                "media_urls": sorted(media_urls),
            }
