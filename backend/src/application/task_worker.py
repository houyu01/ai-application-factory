"""Durable background worker for SQLite-backed generation tasks.

FastAPI ``BackgroundTasks`` only lives for the lifetime of the current
process. This worker is intentionally small and local: tasks are claimed from
SQLite with a lease, so a process restart can resume the same task instead of
losing it. A production deployment can replace this loop with Celery, RQ, or a
managed queue without changing the persisted task contract.
"""

from __future__ import annotations

import logging
import threading

from .game_service import InteractiveGameService, game_service
from .task_service import TaskService, task_service
from ..domain.models import GenerationStatus


logger = logging.getLogger(__name__)


class DurableTaskWorker:
    """Continuously resume durable drama/game tasks after refreshes or restarts.

    The application lifespan starts this worker; it solves the limitation of
    request-scoped FastAPI background tasks by claiming persisted task rows.
    """

    def __init__(
        self,
        drama: TaskService,
        games: InteractiveGameService,
        *,
        idle_seconds: float = 0.75,
    ) -> None:
        self.drama = drama
        self.games = games
        self.idle_seconds = idle_seconds
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop.clear()
        self._thread = threading.Thread(
            target=self._run,
            name="durable-generation-worker",
            daemon=True,
        )
        self._thread.start()
        logger.info("Durable generation worker started")

    def stop(self) -> None:
        self._stop.set()
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=5)
        self._thread = None

    def _run(self) -> None:
        while not self._stop.is_set():
            did_work = False
            try:
                task = self.drama.repository.claim_next_runnable_task()
                if task is not None:
                    did_work = True
                    self._run_drama_task(task)
            except Exception:
                logger.exception("Durable drama task worker iteration failed")

            try:
                task = self.games.repository.claim_next_runnable_task()
                if task is not None:
                    did_work = True
                    self._run_game_task(task)
            except Exception:
                logger.exception("Durable game task worker iteration failed")

            if not did_work:
                self._stop.wait(self.idle_seconds)

    def _run_drama_task(self, task: dict) -> None:
        try:
            current = self.drama.repository.get_task(task["id"])
            if current is None or current.get("status") != GenerationStatus.GENERATING.value:
                return
            self.drama.resume_task(task)
        except Exception as exc:
            # Provider polling failures are retryable. The provider task id is
            # already durable, so the next worker iteration can continue the
            # same remote task after a transient network or service failure.
            if task.get("type") == "shot_video" and task.get("provider_task_id"):
                logger.warning("Video polling failed; retrying task %s: %s", task["id"], exc)
                self.drama.repository.reschedule_task(task["id"], delay_seconds=10)
                self.drama.repository.update_task_progress(
                    task["id"], error_message=str(exc)
                )
                return
            logger.exception("Durable drama task %s failed", task.get("id"))
            self.drama.repository.update_task_status(
                task["id"], GenerationStatus.FAILED,
                error_message=str(exc),
            )

    def _run_game_task(self, task: dict) -> None:
        try:
            self.games.resume_task(task)
        except Exception as exc:
            logger.exception("Durable game task %s failed", task.get("id"))
            self.games.repository.update_task_status(
                task["id"], GenerationStatus.FAILED, error_message=str(exc)
            )


durable_task_worker = DurableTaskWorker(task_service, game_service)
