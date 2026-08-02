"""Gateway layer for short-drama API use cases."""

from fastapi import BackgroundTasks

from .task_service import TaskService, task_service


class DramaGateway:
    """Compose the API-facing workflow without owning persistence details."""

    def __init__(self, service: TaskService) -> None:
        self.service = service

    def create_project(self, payload, background_tasks: BackgroundTasks):
        project = self.service.create_project(payload)
        background_tasks.add_task(
            self.service.decompose_project, project["task_id"], project["id"]
        )
        return project


drama_gateway = DramaGateway(task_service)
