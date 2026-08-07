"""ORM persistence for durable short-drama generation tasks."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone
from typing import Any
from uuid import uuid4

from sqlalchemy import case, func, or_, select, update

from ..domain.models import GenerationStatus
from .orm_models import GenerationTask
from .repository_common import _json_dump, _json_load, _parse_datetime, utc_now


class DramaRepositoryTaskMixin:
    """Manage restart-safe tasks used by asset, prompt, quality, and video work.

    The API creates rows when a user clicks a generation action; the worker
    claims and updates them so browser refreshes and service restarts retain
    the real status and provider polling state.
    """

    SCRIPT_GENERATION_TASK_TYPES = ("script_decomposition", "script_expansion")

    def create_task(self, drama_id: str, task_type: str, resource_id: str | None = None,
                    input_snapshot: dict[str, Any] | None = None) -> dict[str, Any]:
        timestamp = utc_now()
        trigger_type = {
            "script_decomposition": "DRAMA_BOOTSTRAP", "asset_image": "DRAMA_ASSET",
            "asset_variant_image": "DRAMA_ASSET_VARIANT", "shot_prompt": "DRAMA_SHOT_PROMPT",
            "shot_video": "DRAMA_VIDEO", "cover_image": "DRAMA_COVER",
            "script_expansion": "DRAMA_SCRIPT_EXPANSION", "asset_image_batch": "DRAMA_ASSET_BATCH",
            "shot_reference_image_batch": "DRAMA_ASSET_BATCH",
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

    def create_active_task(
        self, drama_id: str, task_type: str, resource_id: str | None = None,
        input_snapshot: dict[str, Any] | None = None,
    ) -> tuple[dict[str, Any], bool]:
        """Atomically create or return an active task for one user action.

        Screenplay continuation uses this to make repeated button clicks
        idempotent before the worker has a chance to claim the new task.
        """

        timestamp = utc_now()
        job_id = f"{drama_id}:{resource_id or task_type}"
        trigger_type = {
            "script_expansion": "DRAMA_SCRIPT_EXPANSION",
            "asset_image_batch": "DRAMA_ASSET_BATCH", "shot_reference_image_batch": "DRAMA_ASSET_BATCH",
        }.get(task_type, task_type.upper())
        with self.database.task_claim_lock, self.database.session() as session:
            conditions = [
                GenerationTask.drama_id == drama_id,
                GenerationTask.type == task_type,
                GenerationTask.status == GenerationStatus.GENERATING.value,
                GenerationTask.resource_id.is_(None) if resource_id is None else GenerationTask.resource_id == resource_id,
            ]
            active = session.scalars(
                select(GenerationTask).where(*conditions).order_by(GenerationTask.created_at.desc())
            ).first()
            if active is not None:
                return self._task_from_row(active), False
            task_no = len(session.scalars(select(GenerationTask).where(GenerationTask.job_id == job_id)).all()) + 1
            task = GenerationTask(
                id=str(uuid4()), drama_id=drama_id, type=task_type, job_id=job_id,
                task_no=task_no, trigger_type=trigger_type, resource_id=resource_id,
                status=GenerationStatus.GENERATING.value,
                input_snapshot_json=_json_dump(input_snapshot) if input_snapshot is not None else None,
                created_at=timestamp, started_at=timestamp,
            )
            session.add(task)
            session.flush()
            return self._task_from_row(task), True

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

    def project_generation_queue(self) -> dict[str, dict[str, int | str]]:
        """Project the bootstrap-task FIFO into list-card queue metadata.

        The project library calls this while rendering a new short drama.  It
        reads the same persisted task order the worker claims, so a restart
        cannot reset or reorder the position shown to the creator.
        """

        now = datetime.now(timezone.utc)
        with self.database.session() as session:
            tasks = session.scalars(
                select(GenerationTask)
                .where(
                    GenerationTask.type == "script_decomposition",
                    GenerationTask.status == GenerationStatus.GENERATING.value,
                )
                .order_by(GenerationTask.created_at, GenerationTask.id)
            ).all()

        queue: dict[str, dict[str, int | str]] = {}
        for position, task in enumerate(tasks, start=1):
            lease_until = _parse_datetime(task.poll_lease_until)
            # ``update_task_progress`` intentionally releases the lease while
            # a long-running provider stream continues. A live task therefore
            # cannot use its current lease as the sole processing signal.
            # Persisted progress or stage text proves the worker has begun it;
            # only untouched tasks are still waiting for a worker slot.
            processing = bool(
                (lease_until and lease_until > now)
                or int(task.progress or 0) > 0
                or str(task.stage or "").strip()
            )
            queue[task.drama_id] = {
                "queue_position": position,
                "queue_state": "processing" if processing else "queued",
            }
        return queue

    def claim_next_runnable_task(
        self,
        lease_seconds: int = 60,
        task_types: set[str] | None = None,
        max_active_tasks: int | None = None,
    ) -> dict[str, Any] | None:
        """Atomically claim one runnable task from an optional model-specific queue.

        New-drama screenplay and storyboard work ranks ahead of unclaimed
        prompt and quality work in the shared language-model queue. Claimed
        tasks retain their leases, so priority never interrupts active work.
        """

        now = datetime.now(timezone.utc)
        now_value = now.isoformat()
        lease_until = (now + timedelta(seconds=lease_seconds)).isoformat()
        lease_token = str(uuid4())
        runnable = (
            GenerationTask.status == GenerationStatus.GENERATING.value,
            (GenerationTask.next_poll_at.is_(None) | (GenerationTask.next_poll_at <= now_value)),
            (GenerationTask.poll_lease_until.is_(None) | (GenerationTask.poll_lease_until <= now_value)),
        )
        normalized_types = tuple(sorted(task_types or ()))
        if task_types is not None and not normalized_types:
            return None
        if normalized_types:
            runnable += (GenerationTask.type.in_(normalized_types),)
        with self.database.task_claim_lock, self.database.session() as session:
            if max_active_tasks is not None and normalized_types:
                active_count = session.scalar(select(func.count()).where(
                    GenerationTask.status == GenerationStatus.GENERATING.value,
                    GenerationTask.type.in_(normalized_types),
                    or_(
                        GenerationTask.provider_task_id.is_not(None),
                        GenerationTask.poll_lease_until > now_value,
                    ),
                )) or 0
                if active_count >= max(1, max_active_tasks):
                    runnable += (GenerationTask.provider_task_id.is_not(None),)
            task_id = session.scalar(
                select(GenerationTask.id)
                .where(*runnable)
                .order_by(
                    case(
                        (GenerationTask.type.in_(self.SCRIPT_GENERATION_TASK_TYPES), 0),
                        else_=1,
                    ),
                    GenerationTask.next_poll_at,
                    GenerationTask.created_at,
                    GenerationTask.id,
                )
                .limit(1)
            )
            if task_id is None:
                return None
            claimed = session.execute(
                update(GenerationTask)
                .where(GenerationTask.id == task_id, *runnable)
                .values(
                    poll_lease_token=lease_token,
                    poll_lease_until=lease_until,
                    poll_attempts=GenerationTask.poll_attempts + 1,
                )
            )
            if claimed.rowcount != 1:
                return None
            task = session.get(GenerationTask, task_id)
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
            if task.status == GenerationStatus.CANCELLED.value:
                return self._task_from_row(task)
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
            if task.status == GenerationStatus.CANCELLED.value:
                return self._task_from_row(task)
            task.input_snapshot_json = _json_dump(snapshot)
            session.flush()
            return self._task_from_row(task)

    def reschedule_task(self, task_id: str, delay_seconds: int = 3) -> dict[str, Any]:
        next_poll_at = (datetime.now(timezone.utc) + timedelta(seconds=delay_seconds)).isoformat()
        return self.update_task_progress(task_id, next_poll_at=next_poll_at)

    def retry_failed_task(self, task_id: str) -> dict[str, Any]:
        """Requeue a failed durable task while retaining its restart snapshot."""

        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            if task is None:
                raise KeyError(f"Task not found: {task_id}")
            if task.status != GenerationStatus.FAILED.value:
                raise ValueError("只有生成失败的任务可以重试")
            task.status = GenerationStatus.GENERATING.value
            task.progress = 0
            task.stage = "等待从已保存内容重试"
            task.error_message = None
            task.result_json = None
            task.output_result_json = None
            task.duration_ms = None
            task.completed_at = None
            task.finished_at = None
            task.next_poll_at = None
            task.poll_attempts = 0
            task.poll_lease_token = None
            task.poll_lease_until = None
            session.flush()
            return self._task_from_row(task)

    def cancel_task(self, task_id: str, *, stage: str = "任务已取消") -> dict[str, Any]:
        """Cancel a generating task and immediately clear its worker lease.

        Screenplay and video cancellation flows call this before any remote
        cleanup. Marking cancellation durably first prevents another worker
        from claiming the task while the active worker observes the new state.
        """

        with self.database.session() as session:
            task = session.get(GenerationTask, task_id)
            if task is None:
                raise KeyError(f"Task not found: {task_id}")
            if task.status != GenerationStatus.GENERATING.value:
                raise ValueError("任务未在运行，无法取消")
            completed_at = utc_now()
            task.status = GenerationStatus.CANCELLED.value
            task.stage = stage
            task.error_message = None
            task.completed_at = completed_at
            task.finished_at = completed_at
            task.next_poll_at = None
            task.poll_lease_token = None
            task.poll_lease_until = None
            if task.started_at:
                started = _parse_datetime(task.started_at)
                finished = _parse_datetime(completed_at)
                if started and finished:
                    task.duration_ms = max(0, int((finished - started).total_seconds() * 1000))
            session.flush()
            return self._task_from_row(task)

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

    def list_active_tasks(self, drama_id: str, task_type: str) -> list[dict[str, Any]]:
        """Return every active task of one type for a project-scoped bulk action."""

        with self.database.session() as session:
            tasks = session.scalars(select(GenerationTask).where(
                GenerationTask.drama_id == drama_id,
                GenerationTask.type == task_type,
                GenerationTask.status == GenerationStatus.GENERATING.value,
            ).order_by(GenerationTask.created_at, GenerationTask.id)).all()
            return [self._task_from_row(task) for task in tasks]

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
            if task.status == GenerationStatus.CANCELLED.value and status is not GenerationStatus.CANCELLED:
                return self._task_from_row(task)
            if status is GenerationStatus.GENERATING and task.started_at is None:
                task.started_at = utc_now()
            if status in (GenerationStatus.SUCCEEDED, GenerationStatus.FAILED, GenerationStatus.CANCELLED):
                task.completed_at = utc_now()
                task.finished_at = task.completed_at
                if status is not GenerationStatus.CANCELLED:
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
