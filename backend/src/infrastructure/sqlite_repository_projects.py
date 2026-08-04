"""ORM persistence for short-drama projects and their project-level settings."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import delete, desc, select

from ..domain.models import GenerationStatus, ProjectCreate
from .orm_models import (
    DramaAsset,
    DramaShot,
    DramaShotVersion,
    GenerationTask,
    ShortDrama,
)
from .repository_common import _json_dump, _json_load, utc_now


class DramaRepositoryProjectMixin:
    """Persist a drama project before starting work and expose its aggregate view.

    The API gateway calls these methods when the user creates, lists, opens,
    edits, or deletes a project. Keeping project mutations in ORM sessions
    prevents orphaned shots, assets, versions, and durable tasks.
    """

    def create_drama_with_task(self, payload: ProjectCreate) -> tuple[dict[str, Any], dict[str, Any]]:
        """Create the project and its initial decomposition task atomically."""

        values = payload.model_dump()
        timestamp = utc_now()
        drama = ShortDrama(
            id=str(uuid4()),
            name=values["name"],
            script=values["script"],
            ratio=values["ratio"],
            style=values["style"],
            theme=values["theme"],
            language_model=values["language_model"],
            multimodal_model=values["multimodal_model"],
            video_model=values.get("video_model", "doubao-seedance-2.0"),
            resolution=values.get("resolution", "720p"),
            video_public_prompt=values.get("video_public_prompt", ""),
            asset_public_prompts_json=_json_dump(values.get("asset_public_prompts", {})),
            shot_constraints_json=_json_dump(values.get("shot_constraints", {})),
            status=GenerationStatus.NOT_GENERATED.value,
            created_at=timestamp,
            updated_at=timestamp,
        )
        task = GenerationTask(
            id=str(uuid4()),
            drama_id=drama.id,
            type="script_decomposition",
            job_id=drama.id,
            task_no=1,
            trigger_type="DRAMA_BOOTSTRAP",
            status=GenerationStatus.NOT_GENERATED.value,
            input_snapshot_json=_json_dump(
                {
                    "drama_id": drama.id,
                    "script": values["script"],
                    "language_model": values["language_model"],
                }
            ),
            created_at=timestamp,
        )
        with self.database.session() as session:
            # The existing SQLite schema enforces generation_tasks.drama_id as
            # a foreign key. Flush the owning drama first: this makes the
            # insert order explicit even for databases created before ORM
            # relationship metadata was introduced.
            session.add(drama)
            session.flush()
            session.add(task)
            session.flush()
            return self._drama_from_row(drama), self._task_from_row(task)

    def list_dramas(self) -> list[dict[str, Any]]:
        """Load the project list and aggregate its normalized shot/asset rows."""

        with self.database.session() as session:
            dramas = session.scalars(select(ShortDrama).order_by(desc(ShortDrama.created_at))).all()
            shots = session.scalars(
                select(DramaShot).order_by(
                    DramaShot.drama_id,
                    DramaShot.episode_sort_order,
                    DramaShot.episode_name,
                    DramaShot.shot_index,
                    DramaShot.created_at,
                )
            ).all()
            assets = session.scalars(
                select(DramaAsset).order_by(DramaAsset.drama_id, DramaAsset.created_at, DramaAsset.id)
            ).all()

        shots_by_drama: dict[str, list[dict[str, Any]]] = {}
        for row in shots:
            item = self._shot_from_row(row)
            shots_by_drama.setdefault(item["drama_id"], []).append(item)
        assets_by_drama: dict[str, list[dict[str, Any]]] = {}
        for row in assets:
            item = self._asset_from_row(row)
            assets_by_drama.setdefault(item["drama_id"], []).append(item)

        result = []
        for row in dramas:
            item = self._drama_from_row(row)
            item["shots"] = shots_by_drama.get(item["id"], [])
            item["assets"] = assets_by_drama.get(item["id"], [])
            item["episodes"] = self._aggregate_episodes(item["shots"])
            result.append(item)
        return result

    def get_drama(self, drama_id: str) -> dict[str, Any] | None:
        """Load one complete project aggregate for the detail editor."""

        with self.database.session() as session:
            drama = session.get(ShortDrama, drama_id)
            if drama is None:
                return None
            assets = session.scalars(
                select(DramaAsset).where(DramaAsset.drama_id == drama_id).order_by(DramaAsset.created_at, DramaAsset.id)
            ).all()
            shots = session.scalars(
                select(DramaShot)
                .where(DramaShot.drama_id == drama_id)
                .order_by(DramaShot.episode_sort_order, DramaShot.episode_name, DramaShot.shot_index, DramaShot.created_at)
            ).all()
            versions = session.scalars(
                select(DramaShotVersion)
                .where(DramaShotVersion.drama_id == drama_id)
                .order_by(DramaShotVersion.shot_id, desc(DramaShotVersion.version_no))
            ).all()
            tasks = session.scalars(
                select(GenerationTask).where(GenerationTask.drama_id == drama_id).order_by(GenerationTask.created_at)
            ).all()

        item = self._drama_from_row(drama)
        item["assets"] = [self._asset_from_row(row) for row in assets]
        shot_items = [self._shot_from_row(row) for row in shots]
        versions_by_shot: dict[str, list[dict[str, Any]]] = {}
        for row in versions:
            versions_by_shot.setdefault(row.shot_id, []).append(self._shot_version_from_row(row))
        for shot in shot_items:
            shot["versions"] = versions_by_shot.get(str(shot["id"]), [])
        item["shots"] = shot_items
        item["episodes"] = self._aggregate_episodes(shot_items)
        item["tasks"] = [self._task_from_row(row) for row in tasks]
        return item

    def drama_exists(self, drama_id: str) -> bool:
        """Check project ownership without loading assets, shots, or tasks."""
        with self.database.session() as session:
            return session.get(ShortDrama, drama_id) is not None

    def delete_drama(self, drama_id: str) -> None:
        """Delete a project and all owned rows, including historical versions."""

        with self.database.session() as session:
            if session.get(ShortDrama, drama_id) is None:
                raise KeyError(f"Project not found: {drama_id}")
            for model in (GenerationTask, DramaShotVersion, DramaShot, DramaAsset):
                session.execute(delete(model).where(model.drama_id == drama_id))
            session.execute(delete(ShortDrama).where(ShortDrama.id == drama_id))

    def update_model_selection(self, drama_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Save the model names selected for this project."""

        allowed = {"language_model", "multimodal_model", "video_model"}
        updates = {key: str(value).strip() for key, value in values.items() if key in allowed and str(value).strip()}
        if not updates:
            raise ValueError("No model fields to update")
        with self.database.session() as session:
            row = session.get(ShortDrama, drama_id)
            if row is None:
                raise KeyError(f"Project not found: {drama_id}")
            for key, value in updates.items():
                setattr(row, key, value)
            row.updated_at = utc_now()
        return self.get_drama(drama_id) or {}

    def update_project_parameters(self, drama_id: str, values: dict[str, Any]) -> dict[str, Any]:
        """Save ratio, style, theme, resolution, constraints, and video prompt settings."""

        allowed = {"ratio", "style", "theme", "resolution", "video_public_prompt"}
        updates = {key: value for key, value in values.items() if key in allowed and value is not None}
        if values.get("shot_constraints") is not None:
            updates["shot_constraints_json"] = _json_dump(values["shot_constraints"])
        with self.database.session() as session:
            row = session.get(ShortDrama, drama_id)
            if row is None:
                raise KeyError(f"Project not found: {drama_id}")
            for key, value in updates.items():
                setattr(row, key, value)
            row.updated_at = utc_now()
        return self.get_drama(drama_id) or {}

    def update_video_public_prompt(self, drama_id: str, video_public_prompt: str) -> dict[str, Any]:
        """Save the video prompt shared by all shots."""

        with self.database.session() as session:
            row = session.get(ShortDrama, drama_id)
            if row is None:
                raise KeyError(f"Project not found: {drama_id}")
            row.video_public_prompt = video_public_prompt.strip()
            row.updated_at = utc_now()
        return self.get_drama(drama_id) or {}

    def update_asset_public_prompt(self, drama_id: str, asset_type: str, public_prompt: str) -> dict[str, Any]:
        """Save one independent character, scene, prop, or placeholder prompt."""

        with self.database.session() as session:
            row = session.get(ShortDrama, drama_id)
            if row is None:
                raise KeyError(f"Project not found: {drama_id}")
            prompts = _json_load(row.asset_public_prompts_json, {})
            if not isinstance(prompts, dict):
                prompts = {}
            prompts[asset_type] = public_prompt.strip()
            row.asset_public_prompts_json = _json_dump(prompts)
            row.updated_at = utc_now()
        return self.get_drama(drama_id) or {}

    def set_drama_status(self, drama_id: str, status: GenerationStatus) -> None:
        """Persist the aggregate status shown on the project list and detail page."""

        with self.database.session() as session:
            row = session.get(ShortDrama, drama_id)
            if row is None:
                raise KeyError(f"Project not found: {drama_id}")
            row.status = status.value
            row.updated_at = utc_now()

    def _require_drama(self, drama_id: str) -> dict[str, Any]:
        item = self.get_drama(drama_id)
        if item is None:
            raise KeyError(f"Project not found: {drama_id}")
        return item
