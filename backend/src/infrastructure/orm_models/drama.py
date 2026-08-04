"""ORM models for short-drama assets, prompts, shots, and durable tasks."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from .base import ORMBase


class ShortDrama(ORMBase):
    """A short-drama project and its project-level generation configuration."""

    __tablename__ = "short_dramas"
    __table_args__ = {"comment": "Short-drama projects and their global generation settings."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable project id; read by every project API and never changed.")
    name: Mapped[str] = mapped_column(String(200), comment="Project display name; changed when the user renames a drama.")
    script: Mapped[str] = mapped_column(Text, comment="Original script; read by decomposition and prompt generation, changed only by project editing.")
    ratio: Mapped[str] = mapped_column(String(40), comment="Video aspect ratio; read by image/video generation and changed in global parameters.")
    style: Mapped[str] = mapped_column(String(120), comment="Visual style; read by every prompt and changed in global parameters.")
    theme: Mapped[str] = mapped_column(String(120), comment="Narrative theme; read by prompt planning and changed in global parameters.")
    language_model: Mapped[str] = mapped_column(String(200), comment="Selected text model; read when LLM tasks start and changed in project settings.")
    multimodal_model: Mapped[str] = mapped_column(String(200), comment="Selected image model; read by asset image tasks and changed in project settings.")
    video_model: Mapped[str] = mapped_column(String(200), default="doubao-seedance-2.0", server_default="doubao-seedance-2.0", comment="Selected video model; read by shot video tasks and changed in project settings.")
    resolution: Mapped[str] = mapped_column(String(40), default="720p", server_default="720p", comment="Output resolution; read by media generation and changed in global parameters.")
    video_public_prompt: Mapped[str] = mapped_column(Text, default="", server_default="", comment="Project-wide video prompt; read before every shot video prompt and edited by the user.")
    asset_public_prompts_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Per-asset-type public prompts; read for asset generation and changed from the asset prompt panel.")
    shot_constraints_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Subtitle/music constraints; read when shot prompts are built and changed in global parameters.")
    status: Mapped[str] = mapped_column(String(40), comment="Project generation status; changed by durable task completion.")
    shots_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Legacy snapshot of shots; read for compatibility and updated when decomposition is saved.")
    assets_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Legacy snapshot of assets; read for compatibility and updated when decomposition is saved.")
    historical_videos_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Project video history snapshot; read by list/detail APIs and appended after successful video generation.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Creation timestamp; read for sorting and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last project update timestamp; changed by every project mutation.")


class DramaAsset(ORMBase):
    """A character, scene, prop, or placeholder image asset belonging to a drama."""

    __tablename__ = "drama_assets"
    __table_args__ = {"comment": "Reusable visual and voice assets referenced by drama shots."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable asset id; embedded in rich prompts and never changed.")
    drama_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning drama id; used to scope every asset query and never changed.")
    type: Mapped[str] = mapped_column(String(40), comment="Asset kind: character, scene, prop, or placeholder; changed only by migration-free manual edits.")
    name: Mapped[str] = mapped_column(String(200), comment="Human-readable asset name; changed when the user edits an asset.")
    prompt: Mapped[str] = mapped_column(Text, comment="Asset image prompt; read by image generation and changed by prompt editing.")
    voice_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Optional voice preset id for character dialogue; changed in character settings.")
    image_url: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Current image URL; read by reference pickers and video generation, changed after upload or generation.")
    content_hash: Mapped[str | None] = mapped_column(String(128), nullable=True, index=True, comment="Image content hash used for deduplication; changed when an image is uploaded.")
    source_type: Mapped[str] = mapped_column(String(40), default="generated", server_default="generated", comment="Image origin such as generated or uploaded; changed with asset replacement.")
    image_history_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Historical image URLs and timestamps; read by image history and appended after each image change.")
    variants_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Alternative asset forms; read by variant management and changed when variants are edited.")
    metadata_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Placeholder/layout/provider metadata; read by specialized generation tasks and changed by those tasks.")
    status: Mapped[str] = mapped_column(String(40), comment="Asset task status; changed when image generation or upload starts and finishes.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Asset creation timestamp; read for ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last asset update timestamp; changed by asset edits and image tasks.")


class DramaShot(ORMBase):
    """One episode-aware shot containing source text, rich prompt, and video history."""

    __tablename__ = "drama_shots"
    __table_args__ = {"comment": "Shot-level script structure, prompt references, quality data, and video history."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable shot id; referenced by editor, prompt, and video APIs.")
    drama_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning drama id; scopes shot reads and never changes.")
    episode_id: Mapped[str] = mapped_column(String(100), comment="Aggregated episode id; used to group shots without a separate episode table.")
    episode_name: Mapped[str] = mapped_column(String(200), comment="Episode display name; changed when decomposition or manual grouping changes.")
    episode_sort_order: Mapped[int] = mapped_column(Integer, default=1, server_default="1", comment="Episode ordering value; read by the editor and changed by regrouping.")
    shot_index: Mapped[int] = mapped_column(Integer, comment="One-based shot order within the episode; read for playback ordering and changed by regrouping.")
    title: Mapped[str] = mapped_column(String(200), comment="Shot title; displayed and edited in the shot editor.")
    original_text: Mapped[str] = mapped_column(Text, comment="Shot-specific source text; read by prompt generation and editable by the user.")
    duration_seconds: Mapped[int] = mapped_column(Integer, default=10, server_default="10", comment="Video duration in seconds; read by prompt/video generation and changed by the shot editor.")
    prompt: Mapped[str] = mapped_column(Text, default="", server_default="", comment="Plain-text video prompt projection; read by providers and regenerated from rich prompt nodes.")
    prompt_rich_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Rich prompt nodes with text and asset references; read by editors and video providers.")
    placeholder_scene_asset_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Scene used for placeholder layout; read by placeholder generation and changed when layout is edited.")
    placeholder_placements_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Relative character positions for a placeholder; read by layout generation and changed in the layout editor.")
    structured_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Structured camera, lighting, audio, and action fields; read by quality checks and provider adapters.")
    quality_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Latest quality-check result; read by editor status and changed after quality checks.")
    quality_status: Mapped[str] = mapped_column(String(40), default="未检查", server_default="未检查", comment="Quality-check label; changed by quality-check tasks.")
    quality_issues_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Quality issue list; read by the editor and replaced after each check.")
    reference_asset_ids_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Selected reference asset ids; read before video generation and changed by the reference picker.")
    prompt_template_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Prompt template id used for this shot; read for version traceability and changed on regeneration.")
    prompt_template_version: Mapped[str] = mapped_column(String(40), default="v1", server_default="v1", comment="Prompt template version snapshot; read for reproducibility and changed on template regeneration.")
    status: Mapped[str] = mapped_column(String(40), comment="Shot task status; changed by prompt/video task transitions.")
    historical_videos_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Shot video versions; read by the preview/history panel and appended after successful generation.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Shot creation timestamp; read for ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last shot update timestamp; changed by editor and generation tasks.")


class DramaShotVersion(ORMBase):
    """A durable version record for one shot video generation attempt."""

    __tablename__ = "drama_shot_versions"
    __table_args__ = {"comment": "Historical shot video versions and provider task progress."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable video version id; referenced by history UI and never changed.")
    drama_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning drama id; scopes version queries and never changed.")
    shot_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning shot id; scopes version history and never changed.")
    task_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Durable task id that generated this version; read for retry/status traceability.")
    version_no: Mapped[int] = mapped_column(Integer, comment="Monotonic version number per shot; read for display and never changed.")
    status: Mapped[str] = mapped_column(String(40), comment="Version generation status; changed by provider polling.")
    prompt: Mapped[str] = mapped_column(Text, default="", server_default="", comment="Prompt snapshot used for this video; read for reproducibility and never changed.")
    prompt_rich_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Rich prompt snapshot; read for reproducibility and never changed.")
    structured_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Structured shot snapshot; read for reproducibility and never changed.")
    quality_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Quality snapshot used for this version; read for audit and never changed.")
    provider_task_id: Mapped[str | None] = mapped_column(String(200), nullable=True, comment="Remote provider task id; read during polling and set when provider accepts work.")
    progress: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Provider progress percentage; updated during polling and read by the UI.")
    video_url: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Final locally or remotely stored video URL; set on success and read for playback/download.")
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Failure detail; set on failure and read by task/version status UI.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Version creation timestamp; read for history ordering and never changed.")
    completed_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Completion timestamp; set when provider work succeeds or fails.")


class PromptTemplate(ORMBase):
    """Versioned prompt template used to make generated prompt structure editable."""

    __tablename__ = "prompt_templates"
    __table_args__ = {"comment": "Editable prompt and quality-check templates with active versions."}

    id: Mapped[str] = mapped_column(String(200), primary_key=True, comment="Template id; referenced by generated shots and never changed.")
    scope: Mapped[str] = mapped_column(String(80), index=True, comment="Template domain such as drama; read when selecting a template.")
    name: Mapped[str] = mapped_column(String(120), comment="Template name; read by template management and never changed for a version.")
    version: Mapped[str] = mapped_column(String(40), comment="Template version label; read for reproducibility and never changed.")
    template_text: Mapped[str] = mapped_column(Text, comment="Template body; read by prompt/quality generation and changed by new versions.")
    metadata_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Template metadata; read by editors and changed with a new version.")
    active: Mapped[int] = mapped_column(Integer, default=1, server_default="1", comment="Whether this version is selectable; changed when a new version is activated.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Template creation timestamp; read for version order and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last template update timestamp; changed when activation changes.")


class GenerationTask(ORMBase):
    """Durable drama task that survives browser refreshes and service restarts."""

    __tablename__ = "generation_tasks"
    __table_args__ = {"comment": "Durable decomposition, asset, prompt, quality, and video task state."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable task id; polled by the frontend and never changed.")
    drama_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning drama id; scopes task queries and never changed.")
    type: Mapped[str] = mapped_column(String(80), comment="Task operation type; read by the worker to choose a handler and never changed.")
    job_id: Mapped[str] = mapped_column(String(100), default="", server_default="", comment="Logical batch/job id; read for grouping related tasks.")
    task_no: Mapped[int] = mapped_column(Integer, default=1, server_default="1", comment="Sequence number within a job; read for ordering.")
    trigger_type: Mapped[str] = mapped_column(String(80), default="GENERIC", server_default="GENERIC", comment="Frontend action that created the task; read for audit and never changed.")
    resource_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Asset or shot id affected by the task; read by status panels.")
    status: Mapped[str] = mapped_column(String(40), comment="Durable task status; changed by worker transitions.")
    input_snapshot_json: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Input snapshot for restart recovery; written at enqueue and read by worker retries.")
    output_result_json: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Provider output snapshot; written on success and read by the UI/audit flow.")
    result_json: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Public task result; written on completion and returned by task APIs.")
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Failure detail; written on failure and returned to the frontend.")
    duration_ms: Mapped[int | None] = mapped_column(Integer, nullable=True, comment="Elapsed task duration; written on completion and read for diagnostics.")
    poll_attempts: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Provider polling attempts; incremented by the worker.")
    poll_lease_token: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Worker lease token; changed when a task is claimed and released.")
    poll_lease_until: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Worker lease expiry; read when claiming and changed on each lease.")
    provider_task_id: Mapped[str | None] = mapped_column(String(200), nullable=True, comment="Remote provider task id; set after provider submission and read during polling.")
    progress: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Task progress percentage; changed by worker and read by loading indicators.")
    stage: Mapped[str] = mapped_column(String(120), default="", server_default="", comment="Human-readable worker stage; changed during long-running tasks.")
    next_poll_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Next provider poll timestamp; changed after each poll.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Task creation timestamp; read for ordering and never changed.")
    started_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="First execution timestamp; set when work starts.")
    finished_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Worker finish timestamp; set when work ends.")
    completed_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Public completion timestamp; set on success or failure.")


class AppSetting(ORMBase):
    """A JSON application setting such as model or storage configuration."""

    __tablename__ = "app_settings"
    __table_args__ = {"comment": "Application-wide JSON settings edited from the configuration page."}

    key: Mapped[str] = mapped_column(String(120), primary_key=True, comment="Setting key; read by providers and changed when configuration is saved.")
    value_json: Mapped[str] = mapped_column(Text, comment="Serialized setting value; read by services and replaced on save.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last setting update timestamp; changed on every save.")


class VoicePreset(ORMBase):
    """Selectable voice description used when building shot narration metadata."""

    __tablename__ = "voice_presets"
    __table_args__ = {"comment": "Built-in and configured voice descriptions for character audio prompts."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Voice preset id; stored on character assets and never changed.")
    name: Mapped[str] = mapped_column(String(160), unique=True, comment="Voice display name; read by selectors and never duplicated.")
    gender: Mapped[str] = mapped_column(String(20), default="", server_default="", comment="Voice gender label; read by selectors and never changed for a preset.")
    prompt: Mapped[str] = mapped_column(Text, default="", server_default="", comment="Voice style description; read when building shot audio prompt text.")
    sort_order: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Selector ordering; changed when preset catalog ordering changes.")
    enabled: Mapped[int] = mapped_column(Integer, default=1, server_default="1", comment="Whether the preset is selectable; changed by catalog administration.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Preset creation timestamp; read for audit and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last preset update timestamp; changed when catalog data changes.")
