"""HTTP routes used by the short-drama cover generation dialog."""

from typing import Literal

from fastapi import HTTPException
from pydantic import BaseModel, Field

from .router_common import api_router, task_service


class DramaCoverGenerationRequest(BaseModel):
    """Cover fields submitted after the user confirms the cover dialog."""

    name: str = Field(min_length=1, max_length=200)
    prompt: str = Field(default="", max_length=10_000)
    ratio: Literal["9:16", "16:9", "1:1", "3:4", "4:3"] = "9:16"
    count: int = Field(default=1, ge=1, le=8)
    character_asset_ids: list[str] = Field(default_factory=list)
    scene_asset_ids: list[str] = Field(default_factory=list)
    extra_reference_asset_ids: list[str] = Field(default_factory=list)


@api_router.post("/projects/{project_id}/covers/generate", status_code=202)
def generate_project_covers(project_id: str, payload: DramaCoverGenerationRequest):
    """Create a durable cover task when the user clicks Generate Cover."""

    try:
        return task_service.enqueue_cover_image(
            project_id,
            name=payload.name,
            prompt=payload.prompt,
            ratio=payload.ratio,
            count=payload.count,
            character_asset_ids=payload.character_asset_ids,
            scene_asset_ids=payload.scene_asset_ids,
            extra_reference_asset_ids=payload.extra_reference_asset_ids,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
