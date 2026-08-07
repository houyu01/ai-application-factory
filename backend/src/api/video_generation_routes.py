"""HTTP route for creating parallel durable short-drama video tasks."""

from fastapi import HTTPException, Request

from ..domain.models import DramaShotVideoBatchRequest, DramaShotVideoBatchResponse
from .router_common import api_router, request_public_media_base_url, task_service


@api_router.post(
    "/projects/{project_id}/shots/{shot_id}/videos",
    response_model=DramaShotVideoBatchResponse,
    status_code=202,
)
def generate_shot_videos(
    project_id: str,
    shot_id: str,
    payload: DramaShotVideoBatchRequest,
    request: Request,
):
    """Create the 1–3 video versions selected beside a shot's duration control.

    The editor calls this after the user presses Generate Video. It returns all
    independent durable tasks so the interface can show every pending version.
    """

    try:
        tasks = task_service.enqueue_shot_videos(
            project_id,
            shot_id,
            payload.count,
            public_media_base_url=request_public_media_base_url(request),
        )
        return {"requested_count": payload.count, "tasks": tasks}
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
