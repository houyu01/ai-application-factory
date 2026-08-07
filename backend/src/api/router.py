import base64
import binascii
import hashlib
import mimetypes
import re
from datetime import datetime, timezone
from typing import Any

from fastapi import BackgroundTasks, HTTPException, Request
from fastapi.responses import FileResponse
from .router_common import api_router, drama_gateway, media_store, request_public_media_base_url, task_service
from ..infrastructure.sqlite_repository_mapping import DramaRepositoryMappingMixin
from ..domain.models import (
    GameEdgeCreate,
    GameEdgeUpdate,
    GameChoiceRequest,
    GameNodeUpdate,
    InteractiveGameCreate,
    DramaAssetCreate,
    DramaAssetUpdate,
    DramaAssetUpload,
    DramaAssetVariantCreate,
    DramaAssetVariantUpdate,
    DramaVideoPublicPromptUpdate,
    DramaAssetPublicPromptUpdate,
    DramaPlaceholderGenerationRequest,
    DramaPlaceholderLayoutUpdate,
    DramaShotUpdate,
    DramaShotCreate,
    ModelSelectionUpdate,
    ProjectParametersUpdate,
    ProjectCreate,
    StorageConfig,
    TaskResponse,
    VoicePreset,
)

@api_router.get("/media/{media_id}")
def get_media(media_id: str):
    """Frontend route: called when the console performs the get media action; returns the persisted result or an asynchronous task status."""
    path = media_store.path_for(media_id)
    if path is None:
        raise HTTPException(status_code=404, detail="Media not found")
    return FileResponse(path, media_type=media_store.content_type(path))

@api_router.get("/projects")
def list_projects():
    """Frontend route: called when the console performs the list projects action; returns the persisted result or an asynchronous task status."""
    return task_service.list_projects()

@api_router.post("/projects", status_code=202)
def create_project(payload: ProjectCreate, background_tasks: BackgroundTasks):
    """Frontend route: called when the console performs the create project action; returns the persisted result or an asynchronous task status."""
    return drama_gateway.create_project(payload, background_tasks)

@api_router.get("/projects/{project_id}")
def get_project(project_id: str, shot_id: str | None = None):
    """Frontend route: called when the console performs the get project action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.get_editor_project(project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


def _task_status_payload(task: dict[str, Any]) -> dict[str, Any]:
    """Expose polling metadata and the bounded screenplay preview for the banner."""
    fields = ("id", "type", "status", "project_id", "resource_id", "progress", "stage", "provider_task_id", "next_poll_at", "created_at", "started_at", "completed_at", "finished_at", "error_message")
    payload = {field: task.get(field) for field in fields}
    snapshot = task.get("input_snapshot")
    preview = snapshot.get("expanded_script_preview") if task.get("type") == "script_decomposition" and isinstance(snapshot, dict) else None
    if isinstance(preview, str):
        payload["input_snapshot"] = {"expanded_script_preview": DramaRepositoryMappingMixin._detail_expanded_preview(preview)}
    return payload


@api_router.get("/projects/{project_id}/tasks")
def list_project_task_statuses(
    project_id: str, status: str = "生成中", since: str | None = None
):
    """Frontend route: poll only generation status while buttons show loading."""
    try:
        if not task_service.repository.drama_exists(project_id):
            raise KeyError(f"Project not found: {project_id}")
        server_time = datetime.now(timezone.utc).isoformat()
        tasks = task_service.repository.list_task_statuses(
            project_id, status=status or None, since=since
        )
        return {
            "project_id": project_id,
            "server_time": server_time,
            "tasks": [_task_status_payload(task) for task in tasks],
        }
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/projects/{project_id}/assets")
def list_project_assets(project_id: str):
    """Frontend route: refresh asset cards after an image task changes state."""
    try:
        if not task_service.repository.drama_exists(project_id):
            raise KeyError(f"Project not found: {project_id}")
        return task_service.repository.list_assets(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/projects/{project_id}/shots")
def list_project_shots(project_id: str):
    """Frontend route: refresh shot data after prompt, quality, or video tasks."""
    try:
        if not task_service.repository.drama_exists(project_id):
            raise KeyError(f"Project not found: {project_id}")
        shots = task_service.repository.list_shots(project_id)
        return {
            "shots": shots,
            "episodes": task_service.repository._aggregate_episodes(shots),
        }
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/projects/{project_id}")
def delete_project(project_id: str):
    """Frontend route: called when the console performs the delete project action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.delete_project(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/models")
def update_project_models(project_id: str, payload: ModelSelectionUpdate):
    """Frontend route: called when the console performs the update project models action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.update_model_selection(
            project_id, payload.model_dump(exclude_none=True)
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/parameters")
def update_project_parameters(project_id: str, payload: ProjectParametersUpdate):
    """Save Global Parameters changes without enqueuing prompt or video generation."""
    try:
        return task_service.repository.update_project_parameters(
            project_id, payload.model_dump(exclude_none=True)
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/video-public-prompt")
def update_project_video_public_prompt(
    project_id: str, payload: DramaVideoPublicPromptUpdate
):
    """Frontend route: called when the console performs the update project video public prompt action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.update_video_public_prompt(
            project_id, payload.video_public_prompt
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/asset-public-prompt")
def update_project_asset_public_prompt(
    project_id: str, payload: DramaAssetPublicPromptUpdate
):
    """Frontend route: called when the console performs the update project asset public prompt action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.update_asset_public_prompt(
            project_id, payload.asset_type, payload.public_prompt
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

@api_router.post("/projects/{project_id}/assets/{asset_id}/image", response_model=TaskResponse, status_code=202)
def generate_asset_image(project_id: str, asset_id: str, background_tasks: BackgroundTasks):
    """Frontend route: called when the console performs the generate asset image action; returns the persisted result or an asynchronous task status."""
    try:
        task = task_service.enqueue("asset_image", project_id, asset_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return task


@api_router.post("/projects/{project_id}/assets")
def create_project_asset(project_id: str, payload: DramaAssetCreate):
    """Frontend route: called when the console performs the create project asset action; returns the persisted result or an asynchronous task status."""
    try:
        if task_service.repository.get_drama(project_id) is None:
            raise KeyError(f"Project not found: {project_id}")
        return task_service.repository.create_asset(
            project_id,
            payload.type,
            payload.name,
            payload.prompt,
            payload.metadata,
            payload.voice_id,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.delete("/projects/{project_id}/assets/{asset_id}")
def delete_project_asset(project_id: str, asset_id: str):
    """Frontend route: called when the console performs the delete project asset action; returns the persisted result or an asynchronous task status."""
    try:
        task_service.repository.delete_asset(project_id, asset_id)
        return {"status": "deleted", "id": asset_id}
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/assets/{asset_id}")
def update_project_asset(project_id: str, asset_id: str, payload: DramaAssetUpdate):
    """Frontend route: called when the console performs the update project asset action; returns the persisted result or an asynchronous task status."""
    try:
        # ``null`` is meaningful here: it explicitly clears a character's
        # voice.  Pydantic otherwise gives omitted and explicit-null fields the
        # same Python value, so preserve that distinction at the API boundary.
        voice_id = payload.voice_id
        if "voice_id" in payload.model_fields_set and voice_id is None:
            voice_id = ""
        return task_service.repository.update_asset(
            project_id,
            asset_id,
            name=payload.name,
            prompt=payload.prompt,
            image_url=payload.image_url,
            voice_id=voice_id,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/assets/{asset_id}/upload")
def upload_project_asset_image(
    project_id: str, asset_id: str, payload: DramaAssetUpload
):
    """Frontend route: called when the console performs the upload project asset image action; returns the persisted result or an asynchronous task status."""
    match = re.fullmatch(r"data:(image/[a-zA-Z0-9.+-]+);base64,(.+)", payload.data_url)
    if not match:
        raise HTTPException(status_code=400, detail="仅支持 base64 图片 data URL")
    content_type, encoded = match.groups()
    try:
        content = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise HTTPException(status_code=400, detail="图片内容无效") from exc
    if len(content) > 15 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="图片不能超过 15MB")
    extension = mimetypes.guess_extension(content_type) or ".png"
    try:
        content_hash = hashlib.sha256(content).hexdigest()
        duplicate = task_service.repository.find_asset_by_content_hash(
            project_id, content_hash
        )
        if duplicate is not None:
            updated = task_service.repository.set_asset_image(
                project_id,
                asset_id,
                str(duplicate.get("image_url") or ""),
                content_hash=content_hash,
            )
            return {**updated, "deduplicated": True, "deduplicated_from": duplicate["id"]}
        image_url = media_store.save(content, extension, content_type=content_type)
        return task_service.repository.set_asset_image(
            project_id,
            asset_id,
            image_url,
            content_hash=content_hash,
            source_type="uploaded",
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/assets/{asset_id}/variants")
def create_project_asset_variant(
    project_id: str, asset_id: str, payload: DramaAssetVariantCreate
):
    """Frontend route: called when the console performs the create project asset variant action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.create_asset_variant(
            project_id, asset_id, payload.name, payload.prompt
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/assets/{asset_id}/variants/{variant_id}")
def update_project_asset_variant(
    project_id: str,
    asset_id: str,
    variant_id: str,
    payload: DramaAssetVariantUpdate,
):
    """Frontend route: called when the console performs the update project asset variant action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.update_asset_variant(
            project_id,
            asset_id,
            variant_id,
            name=payload.name,
            prompt=payload.prompt,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/projects/{project_id}/assets/{asset_id}/variants/{variant_id}")
def delete_project_asset_variant(project_id: str, asset_id: str, variant_id: str):
    """Frontend route: called when the console performs the delete project asset variant action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.repository.delete_asset_variant(project_id, asset_id, variant_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post(
    "/projects/{project_id}/assets/{asset_id}/variants/{variant_id}/image",
    response_model=TaskResponse,
    status_code=202,
)
def generate_project_asset_variant_image(
    project_id: str,
    asset_id: str,
    variant_id: str,
    background_tasks: BackgroundTasks,
):
    """Frontend route: called when the console performs the generate project asset variant image action; returns the persisted result or an asynchronous task status."""
    try:
        task = task_service.enqueue_asset_variant_image(project_id, asset_id, variant_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return task


@api_router.post("/projects/{project_id}/shots/{shot_id}/prompt", response_model=TaskResponse, status_code=202)
def generate_shot_prompt(project_id: str, shot_id: str, background_tasks: BackgroundTasks):
    """Frontend route: called when the console performs the generate shot prompt action; returns the persisted result or an asynchronous task status."""
    try:
        task = task_service.enqueue("shot_prompt", project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return task


@api_router.post("/projects/{project_id}/shots/{shot_id}/video", response_model=TaskResponse, status_code=202)
def generate_shot_video(project_id: str, shot_id: str, request: Request):
    """Frontend route: called by a shot's Generate Video button to create one durable provider task."""
    try:
        return task_service.enqueue("shot_video", project_id, shot_id, public_media_base_url=request_public_media_base_url(request))
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/shots")
def create_project_shot(project_id: str, payload: DramaShotCreate):
    """Create an empty shot directly below the shot whose plus button was clicked."""
    try:
        return task_service.create_shot(
            project_id, payload.after_shot_id,
            title=payload.title, original_text=payload.original_text,
            prompt=payload.prompt, prompt_rich=payload.prompt_rich,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/projects/{project_id}/shots/{shot_id}")
def update_project_shot(project_id: str, shot_id: str, payload: DramaShotUpdate):
    """Save shot fields after edits to text, prompt, duration, or selected references."""
    try:
        return task_service.repository.update_shot(
            project_id, shot_id, **payload.model_dump(exclude_none=True)
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/projects/{project_id}/shots/{shot_id}")
def delete_project_shot(project_id: str, shot_id: str):
    """Delete the selected shot and stop its in-flight generation tasks."""
    try:
        return task_service.delete_shot(project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post(
    "/projects/{project_id}/shots/{shot_id}/auto-match-references",
    response_model=TaskResponse,
    status_code=202,
)
def auto_match_shot_references(project_id: str, shot_id: str):
    """Rebuild the shot prompt and persist the planner-selected references."""
    try:
        return task_service.enqueue("shot_prompt", project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post(
    "/projects/{project_id}/shots/{shot_id}/quality",
    response_model=TaskResponse,
    status_code=202,
)
def generate_shot_quality(project_id: str, shot_id: str):
    """Frontend route: called when the console performs the generate shot quality action; returns the persisted result or an asynchronous task status."""
    try:
        return task_service.enqueue("shot_quality", project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/projects/{project_id}/shots/{shot_id}/versions")
def list_shot_versions(project_id: str, shot_id: str):
    """Frontend route: called when the console performs the list shot versions action; returns the persisted result or an asynchronous task status."""
    try:
        task_service.get_project(project_id)
        if task_service.repository.get_shot(project_id, shot_id) is None:
            raise KeyError(f"Shot not found: {shot_id}")
        return task_service.repository.list_shot_versions(project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


from . import settings_routes  # noqa: F401  # register settings endpoints
from . import game_routes  # noqa: F401  # register game endpoints on the shared router
from . import placeholder_routes  # noqa: F401  # register shot placeholder endpoints

from . import cover_routes  # noqa: F401  # register durable cover-image endpoints
from . import project_metadata_routes  # noqa: F401  # register project metadata endpoints
from . import asset_batch_routes, expanded_script_routes, script_retry_routes, video_history_routes  # noqa: F401  # register screenplay and video-history endpoints
