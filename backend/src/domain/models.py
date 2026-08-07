from datetime import datetime
from enum import Enum
from typing import Any, Literal

from pydantic import BaseModel, Field, model_validator


class GenerationStatus(str, Enum):
    """Status shared by drama decomposition and media generation tasks."""

    NOT_GENERATED = "未生成"
    GENERATING = "生成中"
    SUCCEEDED = "生成成功"
    FAILED = "生成失败"
    CANCELLED = "已取消"


class DramaShotConstraints(BaseModel):
    """Constraints applied to every generated shot in a drama."""

    subtitles: bool = False
    background_music: bool = False


class ProjectCreate(BaseModel):
    """Form payload used when the frontend creates a short-drama project."""
    name: str
    script: str = Field(min_length=10)
    ratio: str = "9:16"
    style: str = "真人风格"
    theme: str = "都市"
    language_model: str = "doubao-seed"
    multimodal_model: str = "doubao-seeddream"
    video_model: str = "doubao-seedance-2.0"
    episode_count: int = Field(default=25, ge=2, le=100)
    enable_web_search: bool = False
    expanded_script_min_chars: int = Field(default=5_000, ge=1, le=1_000_000)
    expanded_script_max_chars: int = Field(default=10_000, ge=1, le=1_000_000)
    shot_script_max_chars: int = Field(default=400, ge=1, le=1_000_000)
    resolution: str = "720p"
    shot_constraints: DramaShotConstraints = Field(default_factory=DramaShotConstraints)
    video_public_prompt: str = ""
    asset_public_prompts: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def validate_expanded_script_range(self) -> "ProjectCreate":
        """Keep the project expansion range valid before a durable task is created."""

        if self.expanded_script_min_chars > self.expanded_script_max_chars:
            raise ValueError("扩写字数最小值不能大于最大值")
        return self


class ModelSelectionUpdate(BaseModel):
    """Project-level model selections, independent from endpoint credentials."""

    language_model: str | None = Field(default=None, min_length=1, max_length=200)
    multimodal_model: str | None = Field(default=None, min_length=1, max_length=200)
    video_model: str | None = Field(default=None, min_length=1, max_length=200)


class ProjectParametersUpdate(BaseModel):
    """Project-level defaults used by explicit future generation actions."""

    ratio: str | None = Field(default=None, min_length=1, max_length=40)
    style: str | None = Field(default=None, min_length=1, max_length=120)
    theme: str | None = Field(default=None, min_length=1, max_length=120)
    resolution: str | None = Field(default=None, min_length=1, max_length=40)
    shot_constraints: DramaShotConstraints | None = None
    video_public_prompt: str | None = None


class ProjectNameUpdate(BaseModel):
    """Editable title submitted from the short-drama detail toolbar."""

    name: str = Field(min_length=1, max_length=200)


class DramaScriptUpdate(BaseModel):
    """Original and expanded screenplay text saved from the project script dialog."""

    script: str = Field(min_length=10)
    expanded_script: str = ""


class GamePlatform(str, Enum):
    """Supported runtime targets for an interactive video game."""
    WECHAT = "微信小游戏"
    MOBILE = "手机原生游戏"
    STEAM = "Steam游戏"


class GameStyle(str, Enum):
    """Visual styles selectable in the interactive-game creation form."""
    LIVE_ACTION = "真人风格"
    ANIME_2D = "2D动漫"
    ANIME_3D = "3D动漫"


class InteractiveGameCreate(BaseModel):
    """Creation settings for an interactive full-motion-video game."""

    name: str = Field(min_length=1, max_length=120)
    script: str = Field(min_length=20)
    platform: GamePlatform = GamePlatform.STEAM
    style: GameStyle = GameStyle.LIVE_ACTION
    success_ending_count: int = Field(default=2, ge=1, le=100)
    failure_ending_count: int = Field(default=30, ge=1, le=200)
    branch_min: int = Field(default=2, ge=2, le=4)
    branch_max: int = Field(default=4, ge=2, le=4)
    node_duration_min: int = Field(default=5, ge=1, le=600)
    node_duration_max: int = Field(default=30, ge=1, le=600)
    language_model: str = "doubao-seed"
    multimodal_model: str = "doubao-seeddream"
    video_model: str = "doubao-seedance-2.0"

    @model_validator(mode="after")
    def validate_ranges(self) -> "InteractiveGameCreate":
        if self.branch_min > self.branch_max:
            raise ValueError("branch_min must be less than or equal to branch_max")
        if self.node_duration_min > self.node_duration_max:
            raise ValueError(
                "node_duration_min must be less than or equal to node_duration_max"
            )
        return self


class GameNodeUpdate(BaseModel):
    """Editable node fields submitted from the game graph editor."""
    title: str | None = Field(default=None, min_length=1, max_length=200)
    original_text: str | None = None
    prompt: str | None = None
    video_url: str | None = None
    status: str | None = None
    duration_seconds: int | None = Field(default=None, ge=1, le=600)
    position_x: int | None = None
    position_y: int | None = None


class GameEdgeCreate(BaseModel):
    """New choice edge submitted when a user links two game nodes."""
    source_node_id: str
    target_node_id: str
    option_text: str = Field(min_length=1, max_length=200)
    sort_order: int = Field(default=1, ge=1)


class GameEdgeUpdate(BaseModel):
    """Editable choice edge fields submitted from the game graph editor."""
    option_text: str | None = Field(default=None, min_length=1, max_length=200)
    target_node_id: str | None = None
    sort_order: int | None = Field(default=None, ge=1)


class GameChoiceRequest(BaseModel):
    """Choice payload submitted when a player selects an outgoing edge."""
    edge_id: str

class TaskResponse(BaseModel):
    """Public durable-task status returned to loading indicators and polling UI."""
    id: str
    type: str
    status: str
    project_id: str
    created_at: datetime
    resource_id: str | None = None
    started_at: datetime | None = None
    completed_at: datetime | None = None
    error_message: str | None = None
    progress: int = Field(default=0, ge=0, le=100)
    stage: str = ""
    provider_task_id: str | None = None
    next_poll_at: datetime | None = None
    warning_message: str | None = None
    input_snapshot: dict[str, Any] | None = None


class DramaShotVideoBatchRequest(BaseModel):
    """Requested parallel output count when the editor generates a shot video."""

    count: int = Field(default=1, ge=1, le=3)


class DramaShotVideoBatchResponse(BaseModel):
    """Independent durable tasks created by one editor-side video request."""

    requested_count: int = Field(ge=1, le=3)
    tasks: list[TaskResponse]


class DramaAssetUpdate(BaseModel):
    """Asset fields edited from the character, scene, or prop drawer."""
    name: str | None = Field(default=None, min_length=1, max_length=200)
    prompt: str | None = None
    image_url: str | None = None
    voice_id: str | None = Field(default=None, max_length=200)


class DramaAssetCreate(BaseModel):
    """Asset fields submitted when the user manually adds a drama resource."""
    type: Literal["character", "scene", "prop", "placeholder", "cover_reference"]
    name: str = Field(min_length=1, max_length=200)
    prompt: str = ""
    voice_id: str | None = Field(default=None, max_length=200)
    metadata: dict[str, Any] = Field(default_factory=dict)


class DramaAssetVariantCreate(BaseModel):
    """Alternative-form fields submitted from an asset's variant editor."""
    name: str = Field(min_length=1, max_length=200)
    prompt: str = ""


class DramaAssetVariantUpdate(BaseModel):
    """Changed alternative-form fields submitted from the variant editor."""
    name: str | None = Field(default=None, min_length=1, max_length=200)
    prompt: str | None = None


class DramaAssetUpload(BaseModel):
    """Data URL payload submitted when the user uploads a reference image."""
    data_url: str = Field(min_length=32, max_length=20_000_000)


class DramaVideoPublicPromptUpdate(BaseModel):
    """Project-wide video prompt edited from the video-prompt modal."""
    video_public_prompt: str = Field(default="", max_length=5000)


class DramaAssetPublicPromptUpdate(BaseModel):
    """Per-asset-type prompt edited from the character/scene/prop modal."""
    asset_type: str = Field(pattern="^(character|scene|prop)$")
    public_prompt: str = Field(default="", max_length=5000)


class DramaPlaceholderPlacement(BaseModel):
    """Relative character placement submitted by the placeholder editor."""
    asset_id: str = Field(min_length=1, max_length=300)
    x: float = Field(default=0.28, ge=0, le=1)
    y: float = Field(default=0.26, ge=0, le=1)
    width: float = Field(default=0.2, gt=0, le=1)
    height: float = Field(default=0.35, gt=0, le=1)
    pose: str = Field(default="", max_length=500)
    note: str = Field(default="", max_length=500)


class DramaPlaceholderLayoutUpdate(BaseModel):
    """Placeholder scene and placements saved from the shot layout editor."""
    shot_id: str = Field(min_length=1, max_length=300)
    scene_asset_id: str = Field(min_length=1, max_length=300)
    placements: list[DramaPlaceholderPlacement] = Field(default_factory=list, max_length=30)


class DramaPlaceholderGenerationRequest(DramaPlaceholderLayoutUpdate):
    """Placeholder layout payload that additionally requests image generation."""
    pass


class DramaShotUpdate(BaseModel):
    """Shot fields changed when users edit content, prompt, duration, or selected references."""
    title: str | None = Field(default=None, min_length=1, max_length=200)
    original_text: str | None = None
    prompt: str | None = None
    prompt_rich: list[dict[str, Any]] | None = None
    reference_asset_ids: list[str] | None = Field(default=None, max_length=200, description="Asset IDs selected for this shot; changed when references are added or removed.")
    duration_seconds: int | None = Field(default=None, ge=3, le=15)
    prompt_template_version: str | None = Field(default=None, pattern=r"^v[0-9]+$")
    first_last_frames: dict[str, Any] | None = Field(default=None, description="Optional first/last frame references used to connect shots within the same episode.")


class DramaShotCreate(BaseModel):
    """Empty shot payload inserted after the currently selected shot."""

    after_shot_id: str = Field(min_length=1, max_length=300)
    title: str = Field(default="未命名分镜", min_length=1, max_length=200)
    original_text: str = ""
    prompt: str = ""
    prompt_rich: list[dict[str, Any]] = Field(default_factory=list)


class DramaVideoGenerationRequest(BaseModel):
    """Optional callback/result URL for providers that generate asynchronously."""

    video_url: str | None = None


class StorageConfig(BaseModel):
    """Persisted media storage settings for local files, TOS, COS, or OSS."""

    provider: Literal["local", "tos", "cos", "oss"] = "local"
    endpoint: str = ""
    bucket: str = ""
    region: str = ""
    secret_id: str = ""
    secret_key: str = ""
    prefix: str = "media"
    public_base_url: str = ""


class VoicePreset(BaseModel):
    """A selectable voice style and the prompt used to describe it."""

    id: str
    name: str
    gender: str = ""
    prompt: str = ""
    sort_order: int = 0
