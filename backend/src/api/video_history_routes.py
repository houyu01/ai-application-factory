"""HTTP routes for short-drama video history and cancellation actions."""

from fastapi import HTTPException

from .router_common import api_router, task_service


@api_router.delete("/projects/{project_id}/shots/{shot_id}/videos/{video_id}")
def delete_project_shot_video(project_id: str, shot_id: str, video_id: str):
    """Remove the video-history card selected by a user's delete action.

    The detail page calls this when the trash icon is clicked on a successful,
    failed, or running video record. The response confirms cleanup of the
    associated task/version metadata and, when present, generated media.
    """

    try:
        return task_service.delete_shot_historical_video(project_id, shot_id, video_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/shots/{shot_id}/video/cancel", status_code=202)
def cancel_project_shot_video(project_id: str, shot_id: str):
    """Stop the selected shot's running video when its dropdown action is clicked.

    The editor exposes this only while a durable ``shot_video`` task is
    generating. The service marks that task cancelled before asking Volcengine
    Ark to delete the remote generation, then returns the terminal task state.
    """

    try:
        return task_service.cancel_shot_video(project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/videos/cancel", status_code=202)
def cancel_project_videos(project_id: str):
    """Cancel all running video jobs when the detail toolbar bulk action is clicked.

    This marks every current ``shot_video`` task cancelled before attempting
    remote provider cleanup, without changing prompts, image tasks, or video history.
    """

    try:
        return task_service.cancel_all_shot_videos(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
