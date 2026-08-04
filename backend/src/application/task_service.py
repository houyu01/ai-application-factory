import os
from typing import Any
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner

from .task_service_project_mixin import TaskServiceProjectMixin
from .task_service_asset_mixin import TaskServiceAssetMixin
from .task_service_worker_mixin import TaskServiceWorkerMixin
from .task_service_provider_mixin import TaskServiceProviderMixin
from .task_service_probe_mixin import TaskServiceModelProbeMixin
from .task_service_prompt_mixin import TaskServicePromptMixin

class TaskService(TaskServiceProjectMixin, TaskServiceAssetMixin, TaskServiceWorkerMixin, TaskServiceProviderMixin, TaskServiceModelProbeMixin, TaskServicePromptMixin):
    """Coordinate durable short-drama project and generation workflows.

    FastAPI gateways call this facade after project, asset, prompt, quality, or
    video actions. The focused mixins hold each workflow slice while this
    facade owns shared repository, planner, and persisted provider settings.
    """

    MODEL_DEFAULTS: dict[str, list[str]] = {
        "language": ["doubao-seed", "gpt-4o-mini"],
        "multimodal": ["doubao-seeddream", "gpt-image-1"],
        "video": ["doubao-seedance-2.0", "sora-2"],
        "audio": ["doubao-voice", "gpt-4o-mini-tts"],
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

    def get_model_api_key(self, kind: str) -> str:
        """Return one provider key only after the settings page requests it."""
        if kind not in self.MODEL_DEFAULTS:
            raise ValueError(f"Unsupported model kind: {kind}")
        configured = self.settings.get(kind, {})
        key = configured.get("api_key")
        if not key and kind == "video":
            key = self.settings.get("multimodal", {}).get("api_key")
        key = key or os.getenv("OPENAI_API_KEY")
        if not key:
            raise KeyError(f"{kind} 模型尚未配置 API Key")
        return str(key)


# The API gateway imports this singleton so all routes and the durable worker
# share one configured repository/service instance.
task_service = TaskService()
