"""HTTP endpoint for resuming failed long-form screenplay expansion."""

from fastapi import HTTPException

from .router_common import api_router, task_service


@api_router.post("/projects/{project_id}/script-decomposition/retry", status_code=202)
def retry_project_script_decomposition(project_id: str):
    """Resume a failed project from its saved story bible and screenplay checkpoint.

    The failed-project banner triggers this endpoint when a creator clicks
    retry. It keeps the same durable task and returns its requeued status so
    the frontend can immediately resume polling.
    """

    try:
        return task_service.retry_script_decomposition(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
