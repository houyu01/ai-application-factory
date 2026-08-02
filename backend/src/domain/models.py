from datetime import datetime
from enum import Enum

from pydantic import BaseModel, Field, model_validator


class GenerationStatus(str, Enum):
    """Status shared by drama decomposition and media generation tasks."""

    NOT_GENERATED = "未生成"
    GENERATING = "生成中"
    SUCCEEDED = "生成成功"
    FAILED = "生成失败"

class ProjectCreate(BaseModel):
    name: str
    script: str = Field(min_length=10)
    ratio: str = "9:16"
    style: str = "真人风格"
    theme: str = "都市"
    language_model: str = "doubao-seed"
    multimodal_model: str = "doubao-seeddream"
    video_model: str = "doubao-seedance-2.0"


class GamePlatform(str, Enum):
    WECHAT = "微信小游戏"
    MOBILE = "手机原生游戏"
    STEAM = "Steam游戏"


class GameStyle(str, Enum):
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
    title: str | None = Field(default=None, min_length=1, max_length=200)
    original_text: str | None = None
    prompt: str | None = None
    video_url: str | None = None
    status: str | None = None
    duration_seconds: int | None = Field(default=None, ge=1, le=600)
    position_x: int | None = None
    position_y: int | None = None


class GameEdgeCreate(BaseModel):
    source_node_id: str
    target_node_id: str
    option_text: str = Field(min_length=1, max_length=200)
    sort_order: int = Field(default=1, ge=1)


class GameEdgeUpdate(BaseModel):
    option_text: str | None = Field(default=None, min_length=1, max_length=200)
    target_node_id: str | None = None
    sort_order: int | None = Field(default=None, ge=1)


class GameChoiceRequest(BaseModel):
    edge_id: str

class TaskResponse(BaseModel):
    id: str
    type: str
    status: str
    project_id: str
    created_at: datetime
    resource_id: str | None = None
    started_at: datetime | None = None
    completed_at: datetime | None = None
    error_message: str | None = None


class DramaAssetUpdate(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=200)
    prompt: str | None = None
    image_url: str | None = None


class DramaShotUpdate(BaseModel):
    title: str | None = Field(default=None, min_length=1, max_length=200)
    original_text: str | None = None
    prompt: str | None = None


class DramaVideoGenerationRequest(BaseModel):
    """Optional callback/result URL for providers that generate asynchronously."""

    video_url: str | None = None
