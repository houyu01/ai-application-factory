import os
from typing import Any
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner

from .task_service_project_mixin import TaskServiceProjectMixin
from .task_service_expansion_mixin import TaskServiceExpansionMixin
from .task_service_decomposition_mixin import TaskServiceDecompositionMixin
from .task_service_asset_mixin import TaskServiceAssetMixin
from .task_service_asset_batch_mixin import TaskServiceAssetBatchMixin
from .task_service_asset_cancellation_mixin import TaskServiceAssetCancellationMixin
from .task_service_worker_mixin import TaskServiceWorkerMixin
from .task_service_provider_mixin import TaskServiceProviderMixin
from .task_service_media_provider_mixin import TaskServiceMediaProviderMixin
from .task_service_model_settings_mixin import TaskServiceModelSettingsMixin
from .task_service_probe_mixin import TaskServiceModelProbeMixin
from .task_service_prompt_mixin import TaskServicePromptMixin
from .task_service_cover_mixin import TaskServiceCoverMixin
from .task_service_detail_mixin import TaskServiceDetailMixin
from .task_service_retry_mixin import TaskServiceRetryMixin
from .task_service_video_cancellation_mixin import TaskServiceVideoCancellationMixin
from .task_service_video_validation_mixin import TaskServiceVideoValidationMixin
from .task_service_video_provider_mixin import TaskServiceVideoProviderMixin

class TaskService(TaskServiceDetailMixin, TaskServiceExpansionMixin, TaskServiceDecompositionMixin, TaskServiceRetryMixin, TaskServiceVideoCancellationMixin, TaskServiceVideoValidationMixin, TaskServiceAssetCancellationMixin, TaskServiceAssetBatchMixin, TaskServiceProjectMixin, TaskServiceAssetMixin, TaskServiceWorkerMixin, TaskServiceModelSettingsMixin, TaskServiceMediaProviderMixin, TaskServiceVideoProviderMixin, TaskServiceProviderMixin, TaskServiceModelProbeMixin, TaskServicePromptMixin, TaskServiceCoverMixin):
    """Coordinate durable short-drama project and generation workflows.

    FastAPI gateways call this facade after project, asset, prompt, quality, or
    video actions. The focused mixins hold each workflow slice while this
    facade owns shared repository, planner, and persisted provider settings.
    """

    MODEL_DEFAULTS: dict[str, list[str]] = {
        "language": ["doubao-seed-1-6-250615", "qwen-plus", "hunyuan-turbos-latest"],
        "multimodal": ["doubao-seedream-4-0-250828", "qwen-image-2.0", "hy-image-v3.0"],
        "video": ["doubao-seedance-2.0", "sora-2"],
        "audio": ["volc.tts_async.default", "qwen3-tts-flash", "mps-sync-dubbing"],
    }
    VIDEO_CREATE_URL_DEFAULT = "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks"
    VIDEO_QUERY_URL_DEFAULT = "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}"

    def __init__(self, repository: SQLiteRepository | None = None, planner: Any | None = None):
        self.repository = repository or SQLiteRepository()
        self.planner = planner or ScriptPlanner()
        persisted_settings = self.repository.get_settings()
        self.settings: dict[str, dict[str, Any]] = {
            key: value
            for key, value in persisted_settings.items()
            if isinstance(value, dict)
        }
        try:
            media_store.configure(self.settings.get("storage", {}))
        except ValueError:
            # A malformed hand-edited setting should not prevent the API from
            # starting; the safe fallback is local storage.
            media_store.configure({"provider": "local"})

    def _refresh_setting(self, kind: str) -> dict[str, Any]:
        """Read the latest persisted model setting before a live request uses it.

        A settings-page API and a background worker can run in separate
        processes against the same SQLite database. This boundary prevents a
        worker from retaining another process's old provider credentials.
        """

        stored = self.repository.get_setting(kind)
        if isinstance(stored, dict):
            self.settings[kind] = stored
            return stored
        current = self.settings.get(kind, {})
        return current if isinstance(current, dict) else {}

    def get_model_api_key(self, kind: str) -> str:
        """Return one provider key only after the settings page requests it."""
        if kind not in self.MODEL_DEFAULTS:
            raise ValueError(f"Unsupported model kind: {kind}")
        configured = self._refresh_setting(kind)
        key = configured.get("api_key")
        if not key and kind == "video":
            key = self.settings.get("multimodal", {}).get("api_key")
        key = key or os.getenv("OPENAI_API_KEY")
        if not key:
            raise KeyError(f"{kind} 模型尚未配置 API Key")
        return str(key)

    def save_model_options(
        self, kind: str, models: list[str], selected_model: str = ""
    ) -> dict[str, Any]:
        """Persist model-list edits made inside a settings-page selector.

        The settings page calls this when a user adds or removes a selectable
        model name. It intentionally does not probe provider connectivity, so
        a failed endpoint probe cannot resurrect an option the user deleted.
        """

        if kind not in self.MODEL_DEFAULTS:
            raise ValueError(f"不支持的模型类型：{kind}")
        normalized_models: list[str] = []
        for value in models:
            name = str(value).strip()
            if name and name not in normalized_models:
                normalized_models.append(name)
        selected = str(selected_model or "").strip()
        if selected not in normalized_models:
            selected = normalized_models[0] if normalized_models else ""
        previous = self._refresh_setting(kind)
        normalized = dict(previous) if isinstance(previous, dict) else {}
        normalized.update({"model": selected, "models": normalized_models})
        self.settings[kind] = normalized
        self.repository.set_setting(kind, normalized)
        return {"status": "saved", **self._public_model_config(kind)}


# The API gateway imports this singleton so all routes and the durable worker
# share one configured repository/service instance.
task_service = TaskService()
