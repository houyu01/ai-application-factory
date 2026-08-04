"""Gateway layer for short-drama API use cases."""

from .task_service import TaskService, task_service


class DramaGateway:
    """Compose the API-facing workflow without owning persistence details."""

    def __init__(self, service: TaskService) -> None:
        self.service = service

    def create_project(self, payload, background_tasks=None):
        # The durable worker consumes the persisted bootstrap task. Keeping
        # this gateway synchronous only creates the database record; it never
        # relies on FastAPI's process-local BackgroundTasks queue.
        return self.service.create_project(payload)


drama_gateway = DramaGateway(task_service)
