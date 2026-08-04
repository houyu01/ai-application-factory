"""ORM persistence for durable short-drama generation tasks."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone
from typing import Any
from uuid import uuid4

from sqlalchemy import or_, select

from ..domain.models import GenerationStatus
from .orm_models import GenerationTask
from .repository_common import _json_dump, _json_load, _parse_datetime, utc_now


class DramaRepositoryTaskMixin:
    """Manage restart-safe tasks used by asset, prompt, quality, and video work.

    The API creates rows when a user clicks a generation action; the worker
    claims and updates them so browser refreshes and service restarts retain
    the real status and provider polling state.
    """

    def create_task(self, drama_id: str, task_type: str, resource_id: str | None = None,
                    input_snapshot: dict[str, Any] | None = None) -> dict[str, Any]:
        timestamp = utc_now()
        trigger_type = {
            "script_decomposition": "DRAMA_BOOTSTRAP", "asset_image": "DRAMA_ASSET",
            "asset_variant_image": "DRAMA_ASSET_VARIANT", "shot_prompt": "DRAMA_SHOT_PROMPT",
            "shot_video": "DRAMA_VIDEO",
        }.get(task_type, task_type.upper())
        job_id = f"{drama_id}:{resource_id or task_type}"
        with self.database.session() as session:
            task_no = len(session.scalars(select(GenerationTask).where(GenerationTask.job_id == job_id)).all()) + 1
            task = GenerationTask(
                id=str(uuid4()), drama_id=drama_id, type=task_type, job_id=job_id,
                task_no=task_no, trigger_type=trigger_type, resource_id=resource_id,
                status=GenerationStatus.NOT_GENERATED.value,
                input_snapshot_json=_json_dump(input_snapshot) if input_snapshot is not None else None,
                created_at=timestamp,
            )
            session.add(task)
            session.flush()
            return self._task_from_row(task)

    def get_task(self, task_id: str) -> dict[str, Any] | None:
        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            return self._task_from_row(task) if task else None

    def list_task_statuses(
        self, drama_id: str, status: str | None = None, since: str | None = None
    ) -> list[dict[str, Any]]:
        """Return active and newly completed tasks without loading project data."""
        with self.database.session() as session:
            conditions = [GenerationTask.drama_id == drama_id]
            if status:
                active_condition = GenerationTask.status == status
                conditions.append(
                    or_(active_condition, GenerationTask.completed_at > since)
                    if since else active_condition
                )
            elif since:
                conditions.append(GenerationTask.completed_at > since)
            tasks = session.scalars(
                select(GenerationTask)
                .where(*conditions)
                .order_by(GenerationTask.created_at, GenerationTask.id)
            ).all()
            return [self._task_from_row(task) for task in tasks]

    def claim_next_runnable_task(self, lease_seconds: int = 60) -> dict[str, Any] | None:
        """Claim one generating task whose lease and polling time have expired."""
        now = datetime.now(timezone.utc)
        now_value = now.isoformat()
        lease_until = (now + timedelta(seconds=lease_seconds)).isoformat()
        with self.database.session() as session:
            task = session.scalars(
                select(GenerationTask).where(
                    GenerationTask.status == GenerationStatus.GENERATING.value,
                    (GenerationTask.next_poll_at.is_(None) | (GenerationTask.next_poll_at <= now_value)),
                    (GenerationTask.poll_lease_until.is_(None) | (GenerationTask.poll_lease_until <= now_value)),
                ).order_by(GenerationTask.next_poll_at, GenerationTask.created_at).limit(1)
            ).first()
            if task is None:
                return None
            task.poll_lease_token = str(uuid4())
            task.poll_lease_until = lease_until
            task.poll_attempts = int(task.poll_attempts or 0) + 1
            session.flush()
            return self._task_from_row(task)

    def update_task_progress(self, task_id: str, *, progress: int | None = None,
                             stage: str | None = None, provider_task_id: str | None = None,
                             next_poll_at: str | None = None, error_message: str | None = None) -> dict[str, Any]:
        """Persist provider progress and release the worker lease."""
        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            if task is None:
                raise KeyError(f"Task not found: {task_id}")
            if progress is not None:
                task.progress = max(0, min(100, int(progress)))
            if stage is not None:
                task.stage = stage
            if provider_task_id is not None:
                task.provider_task_id = provider_task_id
            if next_poll_at is not None:
                task.next_poll_at = next_poll_at
            if error_message is not None:
                task.error_message = error_message
            task.poll_lease_token = None
            task.poll_lease_until = None
            session.flush()
            return self._task_from_row(task)

    def update_task_input_snapshot(self, task_id: str, snapshot: dict[str, Any]) -> dict[str, Any]:
        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            if task is None:
                raise KeyError(f"Task not found: {task_id}")
            task.input_snapshot_json = _json_dump(snapshot)
            session.flush()
            return self._task_from_row(task)

    def reschedule_task(self, task_id: str, delay_seconds: int = 3) -> dict[str, Any]:
        next_poll_at = (datetime.now(timezone.utc) + timedelta(seconds=delay_seconds)).isoformat()
        return self.update_task_progress(task_id, next_poll_at=next_poll_at)

    def get_active_task(self, drama_id: str, task_type: str, resource_id: str | None = None) -> dict[str, Any] | None:
        with self.database.session() as session:
            statement = select(GenerationTask).where(
                GenerationTask.drama_id == drama_id,
                GenerationTask.type == task_type,
                GenerationTask.status == GenerationStatus.GENERATING.value,
            )
            statement = statement.where(GenerationTask.resource_id.is_(None) if resource_id is None else GenerationTask.resource_id == resource_id)
            task = session.scalars(statement.order_by(GenerationTask.created_at.desc()).limit(1)).first()
            return self._task_from_row(task) if task else None

    def get_active_task_by_snapshot(self, drama_id: str, task_type: str, key: str, value: str) -> dict[str, Any] | None:
        with self.database.session() as session:
            tasks = session.scalars(select(GenerationTask).where(
                GenerationTask.drama_id == drama_id, GenerationTask.type == task_type,
                GenerationTask.status == GenerationStatus.GENERATING.value,
            ).order_by(GenerationTask.created_at.desc())).all()
            for task in tasks:
                snapshot = _json_load(task.input_snapshot_json, {})
                if str(snapshot.get(key) or "") == str(value):
                    return self._task_from_row(task)
        return None

    def update_task_status(self, task_id: str, status: GenerationStatus, *,
                           result: dict[str, Any] | None = None,
                           error_message: str | None = None) -> dict[str, Any]:
        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            if task is None:
                raise KeyError(f"Task not found: {task_id}")
            if status is GenerationStatus.GENERATING and task.started_at is None:
                task.started_at = utc_now()
            if status in (GenerationStatus.SUCCEEDED, GenerationStatus.FAILED):
                task.completed_at = utc_now()
                task.finished_at = task.completed_at
                task.progress = 100
                task.next_poll_at = None
            if task.started_at and task.finished_at:
                started = _parse_datetime(task.started_at)
                finished = _parse_datetime(task.finished_at)
                if started and finished:
                    task.duration_ms = max(0, int((finished - started).total_seconds() * 1000))
            task.status = status.value
            if result is not None:
                serialized = _json_dump(result)
                task.result_json = serialized
                task.output_result_json = serialized
            if error_message is not None:
                task.error_message = error_message
            task.poll_lease_token = None
            task.poll_lease_until = None
            session.flush()
            return self._task_from_row(task)
