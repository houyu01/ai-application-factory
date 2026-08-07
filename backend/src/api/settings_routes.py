"""Configuration endpoints for models, prompt templates, voices, and storage."""

from typing import Any

from fastapi import HTTPException
from pydantic import BaseModel, Field

from ..application.task_worker import durable_task_worker
from ..domain.models import StorageConfig, VoicePreset
from .router_common import api_router, task_service


class ModelConfig(BaseModel):
    """Form payload for one provider endpoint and its selectable model names."""

    kind: str = Field(pattern="^(language|multimodal|video|audio)$")
    model: str = ""
    models: list[str] = Field(default_factory=list, max_length=100)
    endpoint: str = ""
    create_url: str = ""
    query_url: str = ""
    provider: str = Field(default="ark", pattern="^(ark|dashscope|tencent)$")
    region: str = ""
    api_key: str = ""
    secret_id: str = ""
    secret_key: str = ""
    app_id: str = ""
    resource_id: str = ""
    voice: str = ""
    video_model: str | None = None
    generation_concurrency: int | None = Field(default=None, ge=1, le=8)


class ModelOptionsUpdate(BaseModel):
    """Selectable model names edited directly inside one settings dropdown."""

    models: list[str] = Field(default_factory=list, max_length=100)
    model: str = ""


class PromptTemplateCreate(BaseModel):
    """Form payload for adding a versioned prompt template."""

    scope: str = "drama"
    name: str = Field(min_length=1, max_length=100)
    version: str = Field(min_length=1, max_length=40)
    template_text: str = Field(min_length=1, max_length=20000)
    metadata: dict[str, Any] = Field(default_factory=dict)


@api_router.get("/prompt-templates")
def list_prompt_templates(
    scope: str = "drama",
    name: str | None = None,
    include_inactive: bool = True,
):
    """The prompt editor calls this when it opens the template version selector."""

    return task_service.repository.list_prompt_templates(scope, name, include_inactive)


@api_router.post("/prompt-templates")
def create_prompt_template(payload: PromptTemplateCreate):
    """The prompt editor calls this after the user saves a new template version."""

    return task_service.repository.create_prompt_template(
        payload.scope, payload.name, payload.version, payload.template_text, payload.metadata
    )


@api_router.put("/settings/models")
def save_model_config(config: ModelConfig):
    """The settings page calls this when a provider endpoint or model list is saved."""

    try:
        saved = task_service.save_model_config(config.model_dump())
        durable_task_worker.set_queue_concurrency(
            config.kind, saved["generation_concurrency"]
        )
        return saved
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.get("/settings/models")
def get_model_configs():
    """The settings page calls this when it loads configured providers and model choices."""

    return task_service.get_model_configs()


@api_router.put("/settings/models/{kind}/options")
def save_model_options(kind: str, payload: ModelOptionsUpdate):
    """The settings selector calls this immediately after adding or deleting an option."""

    try:
        return task_service.save_model_options(kind, payload.models, payload.model)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.get("/settings/models/{kind}/api-key")
def reveal_model_api_key(kind: str):
    """The settings page calls this only after the user clicks an API Key eye button."""

    try:
        return {"kind": kind, "api_key": task_service.get_model_api_key(kind)}
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.get("/settings/voices", response_model=list[VoicePreset])
def get_voice_presets():
    """Project and asset forms call this when they open the voice preset selector."""

    return task_service.repository.list_voice_presets()


@api_router.get("/settings/storage")
def get_storage_config():
    """The settings page calls this to display storage mode without exposing secrets."""

    return task_service.get_storage_config()


@api_router.put("/settings/storage")
def save_storage_config(config: StorageConfig):
    """The settings page calls this after saving local, TOS, COS, or OSS storage."""

    try:
        return task_service.save_storage_config(config.model_dump())
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
