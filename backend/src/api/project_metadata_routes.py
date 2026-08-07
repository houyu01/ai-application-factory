"""HTTP routes for lightweight short-drama project metadata edits."""

from fastapi import HTTPException

from ..domain.models import ProjectNameUpdate
from .router_common import api_router, task_service


@api_router.put("/projects/{project_id}/name")
def update_project_name(project_id: str, payload: ProjectNameUpdate):
    """Rename a drama when the user edits its detail-page title and clicks Save."""

    try:
        return task_service.update_project_name(project_id, payload.name)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
