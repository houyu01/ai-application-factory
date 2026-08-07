"""Durable background worker for SQLite-backed generation tasks.

FastAPI ``BackgroundTasks`` only lives for the lifetime of the current
process. This worker is intentionally small and local: tasks are claimed from
SQLite with a lease, so a process restart can resume the same task instead of
losing it. A production deployment can replace this loop with Celery, RQ, or a
managed queue without changing the persisted task contract.
"""

from __future__ import annotations

import logging
import os
import threading

from .game_service import InteractiveGameService, game_service
from .task_service import TaskService, task_service
from ..domain.models import GenerationStatus
from ..llm_service.planner_expansion_request_mixin import ExpansionCancelledError
from ..llm_service.planner_expansion_mixin import RetryableExpansionError


logger = logging.getLogger(__name__)


class DurableTaskWorker:
    """Continuously resume durable drama/game tasks after refreshes or restarts.

    The application lifespan starts this worker; it solves the limitation of
    request-scoped FastAPI background tasks by claiming persisted task rows.
    """

    EXPANSION_RETRY_ATTEMPTS = 5
    DEFAULT_CONCURRENCY = 2
    MAX_CONCURRENCY = 8
    QUEUE_MODEL_KINDS = {
        "language": "language",
        "image": "multimodal",
        "video": "video",
        "audio": "audio",
    }
    QUEUE_TASK_TYPES = {
        "language": {"script_decomposition", "script_expansion", "shot_prompt", "shot_quality"},
        "image": {"asset_image", "asset_variant_image", "asset_image_batch", "shot_reference_image_batch", "placeholder_image", "cover_image"},
        "video": {"shot_video"},
        "audio": {"audio_generation"},
    }

    def __init__(
        self,
        drama: TaskService,
        games: InteractiveGameService,
        *,
        idle_seconds: float = 0.75,
        concurrency: int | None = None,
    ) -> None:
        self.drama = drama
        self.games = games
        self.idle_seconds = idle_seconds
        self.queue_concurrency = self._resolve_queue_concurrency(concurrency)
        self.concurrency = self.queue_concurrency["video"]
        self._stop = threading.Event()
        self._wake = threading.Event()
        self._thread_lock = threading.Lock()
        self._threads: dict[str, dict[int, threading.Thread]] = {
            queue_name: {} for queue_name in self.QUEUE_MODEL_KINDS
        }

    def _resolve_queue_concurrency(self, configured: int | None) -> dict[str, int]:
        """Read every independent model queue limit from persisted settings."""

        return {
            queue_name: self._resolve_concurrency(queue_name, configured)
            for queue_name in self.QUEUE_MODEL_KINDS
        }

    def _resolve_concurrency(self, queue_name: str, configured: int | None = None) -> int:
        """Resolve one bounded local worker count for a model queue."""

        model_kind = self.QUEUE_MODEL_KINDS[queue_name]
        saved = getattr(self.drama, "settings", {}).get(model_kind, {})
        configured_value = saved.get("generation_concurrency") if isinstance(saved, dict) else None
        candidate = configured if configured is not None else configured_value
        if candidate is None:
            candidate = os.getenv(
                f"GENERATION_{queue_name.upper()}_WORKER_CONCURRENCY",
                os.getenv("GENERATION_WORKER_CONCURRENCY", self.DEFAULT_CONCURRENCY),
            )
        try:
            return min(self.MAX_CONCURRENCY, max(1, int(candidate)))
        except (TypeError, ValueError):
            return self.DEFAULT_CONCURRENCY

    def start(self) -> None:
        """Start one durable worker group for every independently configured queue."""

        with self._thread_lock:
            self._stop.clear()
            self._start_missing_workers_locked()
        logger.info("Started durable generation queues: %s", self.queue_concurrency)

    def set_concurrency(self, configured: int) -> int:
        """Keep the legacy API as an alias for changing video queue concurrency."""

        return self.set_queue_concurrency("video", configured)

    def set_queue_concurrency(self, model_kind: str, configured: int) -> int:
        """Resize one model queue after its Settings card saves a new limit."""

        queue_name = next(
            (name for name, kind in self.QUEUE_MODEL_KINDS.items() if kind == model_kind),
            None,
        )
        if queue_name is None:
            raise ValueError(f"Unsupported generation queue model: {model_kind}")
        resolved = self._resolve_concurrency(queue_name, configured)
        with self._thread_lock:
            self.queue_concurrency[queue_name] = resolved
            self.concurrency = self.queue_concurrency["video"]
            if any(thread.is_alive() for workers in self._threads.values() for thread in workers.values()):
                self._start_missing_workers_locked()
        self._wake.set()
        logger.info("Durable %s queue concurrency changed to %s", queue_name, resolved)
        return resolved

    def _start_missing_workers_locked(self) -> None:
        """Create only missing worker slots while holding the thread-registry lock."""

        for queue_name, concurrency in self.queue_concurrency.items():
            workers = self._threads[queue_name]
            for index in range(1, concurrency + 1):
                current = workers.get(index)
                if current and current.is_alive():
                    continue
                thread = threading.Thread(
                    target=self._run,
                    args=(queue_name, index),
                    name=f"durable-{queue_name}-generation-worker-{index}",
                    daemon=True,
                )
                workers[index] = thread
                thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._wake.set()
        for workers in self._threads.values():
            for thread in list(workers.values()):
                if thread.is_alive():
                    thread.join(timeout=5)
        with self._thread_lock:
            self._threads = {queue_name: {} for queue_name in self.QUEUE_MODEL_KINDS}

    def _run(self, queue_name: str, worker_index: int) -> None:
        while not self._stop.is_set():
            if worker_index > self.queue_concurrency[queue_name]:
                return
            did_work = False
            try:
                task = self.drama.repository.claim_next_runnable_task(
                    task_types=self.QUEUE_TASK_TYPES[queue_name],
                    max_active_tasks=(self.queue_concurrency[queue_name] if queue_name == "video" else None),
                )
                if task is not None:
                    did_work = True
                    self._run_drama_task(task)
            except Exception:
                logger.exception("Durable drama task worker iteration failed")

            if queue_name == "video":
                try:
                    task = self.games.repository.claim_next_runnable_task()
                    if task is not None:
                        did_work = True
                        self._run_game_task(task)
                except Exception:
                    logger.exception("Durable game task worker iteration failed")

            if not did_work:
                self._wake.wait(self.idle_seconds)
                self._wake.clear()

    def _run_drama_task(self, task: dict) -> None:
        try:
            current = self.drama.repository.get_task(task["id"])
            if current is None or current.get("status") != GenerationStatus.GENERATING.value:
                return
            self.drama.resume_task(task)
        except ExpansionCancelledError:
            logger.info("Screenplay expansion cancelled: %s", task["id"])
            return
        except Exception as exc:
            current = self.drama.repository.get_task(task["id"])
            if current is None or current.get("status") == GenerationStatus.CANCELLED.value:
                logger.info("Skipped cancelled drama task: %s", task["id"])
                return
            # Provider polling failures are retryable. The provider task id is
            # already durable, so the next worker iteration can continue the
            # same remote task after a transient network or service failure.
            if task.get("type") in {"script_decomposition", "script_expansion"} and isinstance(exc, RetryableExpansionError):
                # The claimed task snapshot can be older than the row after a
                # retry; read the persisted counter before deciding whether to
                # reschedule or finish the task as failed.
                current = self.drama.repository.get_task(task["id"]) or task
                attempts = int(current.get("poll_attempts") or 0)
                if attempts < self.EXPANSION_RETRY_ATTEMPTS:
                    delay = min(60, 5 * 2 ** max(0, attempts - 1))
                    logger.warning("Screenplay expansion failed temporarily; retrying task %s: %s", task["id"], exc)
                    self.drama.repository.reschedule_task(task["id"], delay_seconds=delay)
                    self.drama.repository.update_task_progress(
                        task["id"],
                        stage=f"语言模型连接暂时不可用，{delay} 秒后从已保存内容重试（{attempts}/{self.EXPANSION_RETRY_ATTEMPTS}）",
                        error_message=str(exc),
                    )
                    return
                self.drama.repository.update_task_status(
                    task["id"], GenerationStatus.FAILED, error_message=str(exc)
                )
                if task.get("type") == "script_decomposition":
                    self.drama.repository.set_drama_status(
                        str(task["drama_id"]), GenerationStatus.FAILED
                    )
                logger.error("Screenplay expansion retry limit reached for %s: %s", task["id"], exc)
                return
            if task.get("type") == "shot_video" and task.get("provider_task_id"):
                logger.warning("Video polling failed; retrying task %s: %s", task["id"], exc)
                self.drama.repository.reschedule_task(task["id"], delay_seconds=10)
                self.drama.repository.update_task_progress(
                    task["id"], error_message=str(exc)
                )
                return
            if task.get("type") == "shot_video" and task.get("resource_id"):
                try:
                    self.drama.fail_shot_video_task(
                        task, str(task["drama_id"]), str(task["resource_id"]), str(exc)
                    )
                    return
                except Exception:
                    logger.exception("Could not persist video failure state for %s", task["id"])
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
