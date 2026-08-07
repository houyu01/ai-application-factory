"""HTTP endpoint for reading a project's persisted long-form screenplay."""

from fastapi import HTTPException

from ..domain.models import DramaScriptUpdate
from .router_common import api_router, task_service


@api_router.get("/projects/{project_id}/expanded-script")
def get_project_expanded_script(project_id: str):
    """Return the expanded screenplay when the detail-toolbar dialog opens it.

    The response intentionally contains only screenplay metadata and content,
    rather than loading the much larger field with every project list or
    detail-editor refresh.
    """

    try:
        return task_service.get_expanded_script(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/expanded-script")
def update_project_expanded_script(project_id: str, payload: DramaScriptUpdate):
    """Save original and expanded screenplay edits from the project script dialog.

    Existing shots are intentionally left unchanged; the creator can review
    and edit source material without silently replacing an active storyboard.
    """

    try:
        return task_service.update_project_scripts(
            project_id, payload.script, payload.expanded_script
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/expanded-script/continue", status_code=202)
def continue_project_expanded_script(project_id: str):
    """Queue another LLM installment when the dialog's 继续扩写 button is clicked.

    The task appends to the saved expanded screenplay and streams its preview
    to the dialog. Existing decomposition, shots, and assets are not rebuilt.
    """

    try:
        return task_service.continue_expanded_script(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/expanded-script/cancel", status_code=202)
def cancel_project_expanded_script(project_id: str):
    """Stop active screenplay expansion or storyboard decomposition from the dialog.

    This persists cancellation before returning so the browser can stop polling;
    the worker then observes the task status, closes any active stream, and
    returns its concurrency slot. This applies throughout the bootstrap flow,
    including the later storyboard-decomposition stage. A click that races a
    recorded failure returns that failed task unchanged, confirming to the
    dialog that it already stopped.
    """

    try:
        return task_service.cancel_script_decomposition(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
