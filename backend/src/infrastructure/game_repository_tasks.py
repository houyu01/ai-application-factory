"""ORM persistence for durable interactive-game tasks."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone
from typing import Any
from uuid import uuid4

from sqlalchemy import and_, or_, select

from ..domain.models import GenerationStatus
from .orm_models import GameTask
from .repository_common import _json_dump, utc_now


class GameRepositoryTaskMixin:
    """Manage restart-safe game planning and video task state.

    The worker calls this slice to enqueue, lease, poll, and finish tasks.
    Keeping leases and snapshots in ORM rows lets a new worker resume after a
    browser refresh or service restart without relying on process memory.
    """

    def create_task(
        self,
        game_id: str,
        task_type: str,
        resource_id: str | None = None,
        input_snapshot: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Create a not-generated task for a game operation."""

        task = GameTask(
            id=str(uuid4()),
            game_id=game_id,
            type=task_type,
            resource_id=resource_id,
            status=GenerationStatus.NOT_GENERATED.value,
            input_snapshot_json=_json_dump(input_snapshot) if input_snapshot is not None else None,
            created_at=utc_now(),
            progress=0,
            stage="",
            poll_attempts=0,
        )
        with self.database.session() as session:
            session.add(task)
            session.flush()
            return self._task_from_row(task)

    def get_task(self, task_id: str) -> dict[str, Any] | None:
        """Return one durable task for the status endpoint."""

        with self.database.session() as session:
            task = session.get(GameTask, task_id)
        return self._task_from_row(task) if task else None

    def claim_next_runnable_task(self, lease_seconds: int = 60) -> dict[str, Any] | None:
        """Lease the oldest runnable generating task for one worker."""

        now = datetime.now(timezone.utc).isoformat()
        lease_until = (datetime.now(timezone.utc) + timedelta(seconds=lease_seconds)).isoformat()
        lease_token = str(uuid4())
        with self.database.session() as session:
            task = session.scalars(
                select(GameTask)
                .where(
                    GameTask.status == GenerationStatus.GENERATING.value,
                    or_(GameTask.next_poll_at.is_(None), GameTask.next_poll_at <= now),
                    or_(GameTask.poll_lease_until.is_(None), GameTask.poll_lease_until <= now),
                )
                .order_by(GameTask.next_poll_at, GameTask.created_at)
                .limit(1)
            ).first()
            if task is None:
                return None
            task.poll_lease_token = lease_token
            task.poll_lease_until = lease_until
            task.poll_attempts = (task.poll_attempts or 0) + 1
            session.flush()
            return self._task_from_row(task)

    def update_task_progress(
        self,
        task_id: str,
        *,
        progress: int | None = None,
        stage: str | None = None,
        next_poll_at: str | None = None,
        error_message: str | None = None,
    ) -> dict[str, Any]:
        """Persist worker progress and release the current polling lease."""

        with self.database.session() as session:
            task = session.get(GameTask, task_id)
            if task is None:
                raise KeyError(f"Game task not found: {task_id}")
            if progress is not None:
                task.progress = max(0, min(100, int(progress)))
            if stage is not None:
                task.stage = stage
            if next_poll_at is not None:
                task.next_poll_at = next_poll_at
            if error_message is not None:
                task.error_message = error_message
            task.poll_lease_token = None
            task.poll_lease_until = None
            session.flush()
            return self._task_from_row(task)

    def get_active_task(
        self, game_id: str, task_type: str, resource_id: str | None = None
    ) -> dict[str, Any] | None:
        """Find the newest generating task for a game resource."""

        with self.database.session() as session:
            task = session.scalars(
                select(GameTask)
                .where(
                    GameTask.game_id == game_id,
                    GameTask.type == task_type,
                    GameTask.resource_id == resource_id,
                    GameTask.status == GenerationStatus.GENERATING.value,
                )
                .order_by(GameTask.created_at.desc())
                .limit(1)
            ).first()
        return self._task_from_row(task) if task else None

    def update_task_status(
        self,
        task_id: str,
        status: GenerationStatus,
        *,
        result: dict[str, Any] | None = None,
        error_message: str | None = None,
    ) -> dict[str, Any]:
        """Persist a terminal or running status with timestamps and result."""

        with self.database.session() as session:
            task = session.get(GameTask, task_id)
            if task is None:
                raise KeyError(f"Game task not found: {task_id}")
            if status is GenerationStatus.GENERATING and task.started_at is None:
                task.started_at = utc_now()
            if status in (GenerationStatus.SUCCEEDED, GenerationStatus.FAILED):
                task.completed_at = utc_now()
                task.progress = 100
                task.next_poll_at = None
            task.status = status.value
            if result is not None:
                task.result_json = _json_dump(result)
            task.error_message = error_message
            task.poll_lease_token = None
            task.poll_lease_until = None
            session.flush()
            return self._task_from_row(task)
