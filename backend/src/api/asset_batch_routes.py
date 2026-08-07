"""HTTP route for the asset drawer's ordered image-batch action."""

from fastapi import HTTPException
from pydantic import BaseModel, Field

from ..domain.models import TaskResponse
from .router_common import api_router, task_service


class DramaAssetImageBatchRequest(BaseModel):
    """IDs displayed in one asset tab when the user clicks Generate All Images."""

    asset_ids: list[str] = Field(min_length=1, max_length=200)


@api_router.post(
    "/projects/{project_id}/assets/images/batch",
    response_model=TaskResponse,
    status_code=202,
)
def generate_asset_images_in_batches(
    project_id: str, payload: DramaAssetImageBatchRequest
):
    """Create a durable five-at-a-time image batch for the selected asset drawer tab."""

    try:
        return task_service.enqueue_asset_image_batch(project_id, payload.asset_ids)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/assets/{asset_type}/images/cancel", status_code=202)
def cancel_asset_image_tasks(project_id: str, asset_type: str):
    """Stop the active images in one asset drawer tab after its cancel button is clicked.

    The character drawer uses this route to cancel only current character base
    and variant images, including its bulk-image coordinator. Scene, prop,
    prompt, placeholder, and video tasks remain untouched.
    """

    try:
        return task_service.cancel_asset_image_tasks(project_id, asset_type)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.post(
    "/projects/{project_id}/shots/{shot_id}/reference-images/generate",
    response_model=TaskResponse,
    status_code=202,
)
def generate_missing_shot_reference_images(project_id: str, shot_id: str):
    """Start the video editor's one-click generation for its missing selected references."""

    try:
        return task_service.enqueue_missing_shot_reference_images(project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
