"""Placeholder-layout endpoints used by the short-drama shot editor."""

from fastapi import HTTPException

from ..domain.models import (
    DramaPlaceholderGenerationRequest,
    DramaPlaceholderLayoutUpdate,
    TaskResponse,
)
from .router_common import api_router, task_service


def _serialized_placements(
    payload: DramaPlaceholderLayoutUpdate,
) -> list[dict[str, object]]:
    """Convert validated placement models before persisting their JSON snapshot."""

    return [placement.model_dump() for placement in payload.placements]


def _ensure_matching_shot(shot_id: str, payload_shot_id: str) -> None:
    """Prevent a dialog payload from changing a different shot than its URL target."""

    if payload_shot_id != shot_id:
        raise HTTPException(status_code=400, detail="请求中的 shot_id 与路径不一致")


@api_router.put("/projects/{project_id}/shots/{shot_id}/placeholder-layout")
def save_placeholder_layout(
    project_id: str,
    shot_id: str,
    payload: DramaPlaceholderLayoutUpdate,
):
    """The placeholder dialog calls this when the user saves its scene and role layout draft."""

    _ensure_matching_shot(shot_id, payload.shot_id)
    try:
        return task_service.repository.update_shot(
            project_id,
            shot_id,
            placeholder_scene_asset_id=payload.scene_asset_id,
            placeholder_placements=_serialized_placements(payload),
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post(
    "/projects/{project_id}/placeholders/image",
    response_model=TaskResponse,
    status_code=202,
)
def generate_placeholder_image(
    project_id: str,
    payload: DramaPlaceholderGenerationRequest,
):
    """The dialog calls this after Generate Placeholder Image to create a durable layout task."""

    try:
        return task_service.enqueue_placeholder_image(
            project_id,
            payload.shot_id,
            payload.scene_asset_id,
            _serialized_placements(payload),
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
