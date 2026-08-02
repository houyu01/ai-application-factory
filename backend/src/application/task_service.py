from datetime import datetime, timezone
from html import escape
import base64
import json
import os
from urllib.request import Request, urlopen
from typing import Any

from ..domain.models import GenerationStatus
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..llm_service.planner import ScriptPlanner
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions


class TaskService:
    """Application service for durable project and generation-task workflows."""

    def __init__(self, repository: SQLiteRepository | None = None, planner: Any | None = None):
        self.repository = repository or SQLiteRepository()
        self.planner = planner or ScriptPlanner()
        self.settings: dict[str, dict[str, Any]] = {}

    def create_project(self, payload: Any) -> dict[str, Any]:
        """Persist the empty drama first, then mark its decomposition task running."""
        project, task = self.repository.create_drama_with_task(payload)
        self.repository.set_drama_status(project["id"], GenerationStatus.GENERATING)
        task = self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)
        project["status"] = GenerationStatus.GENERATING.value
        project["task_id"] = task["id"]
        project["task"] = task
        return project

    def list_projects(self) -> list[dict[str, Any]]:
        return self.repository.list_dramas()

    def get_project(self, project_id: str) -> dict[str, Any]:
        project = self.repository.get_drama(project_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        return project

    def decompose_project(self, task_id: str, project_id: str) -> None:
        try:
            project = self.get_project(project_id)
            if isinstance(self.planner, ScriptPlanner):
                plan = self.planner.plan(
                    project["script"],
                    options=self._provider_options(project, "language"),
                )
            else:
                plan = self.planner.plan(project["script"])
            episodes = plan.get("episodes", [])
            assets = plan.get("assets", [])
            shots = self._flatten_shots(episodes, assets, project["script"])
            self.repository.save_decomposition(project_id, episodes, shots, assets)
            self.repository.set_drama_status(project_id, GenerationStatus.SUCCEEDED)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={"episodes": episodes, "shots": shots, "assets": assets},
            )
        except Exception as exc:
            self.repository.set_drama_status(project_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    @staticmethod
    def _flatten_shots(
        episodes: list[dict[str, Any]], assets: list[dict[str, Any]], script: str
    ) -> list[dict[str, Any]]:
        asset_context = "\n".join(
            f"{asset.get('type', '道具')}：{asset.get('name', '')}；提示词：{asset.get('prompt', '')}"
            for asset in assets
        )
        shots: list[dict[str, Any]] = []
        for episode_index, episode in enumerate(episodes, start=1):
            episode_name = episode.get("name", "第1集")
            for index, shot in enumerate(episode.get("shots", []), start=1):
                original_text = shot.get("original_text", shot.get("script", script[:120]))
                prompt = shot.get("prompt") or (
                    f"剧本：{original_text}\n基础组成元素：\n{asset_context}"
                )
                shots.append(
                    {
                        **shot,
                        "episode_index": episode_index,
                        "episode_name": episode_name,
                        "shot_index": index,
                        "original_text": original_text,
                        "prompt": prompt,
                        "status": GenerationStatus.NOT_GENERATED.value,
                        "historical_videos": [],
                    }
                )
        return shots

    def enqueue(self, kind: str, project_id: str, resource_id: str) -> dict[str, Any]:
        if self.repository.get_drama(project_id) is None:
            raise KeyError(f"Project not found: {project_id}")
        if kind == "asset_image" and self.repository.get_asset(project_id, resource_id) is None:
            raise KeyError(f"Asset not found: {resource_id}")
        if kind in {"shot_prompt", "shot_video"} and self.repository.get_shot(project_id, resource_id) is None:
            raise KeyError(f"Shot not found: {resource_id}")
        task = self.repository.create_task(
            project_id,
            kind,
            resource_id,
            input_snapshot={
                "project_id": project_id,
                "resource_id": resource_id,
                "type": kind,
            },
        )
        if kind == "asset_image":
            self.repository.update_asset_status(resource_id, GenerationStatus.GENERATING)
        if kind in {"shot_prompt", "shot_video"}:
            self.repository.update_shot(
                project_id,
                resource_id,
                status=GenerationStatus.GENERATING,
            )
        return self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)

    def get_task(self, task_id: str) -> dict[str, Any]:
        task = self.repository.get_task(task_id)
        if task is None:
            raise KeyError(f"Task not found: {task_id}")
        return task

    def run_asset_image(self, task_id: str, project_id: str, asset_id: str) -> None:
        try:
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            image_url = self._generate_image_url(project, asset)
            self.repository.update_asset_status(
                asset_id,
                GenerationStatus.SUCCEEDED,
                image_url=image_url,
            )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={"asset_id": asset_id, "image_url": image_url},
            )
        except Exception as exc:
            self.repository.update_asset_status(asset_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def run_shot_prompt(self, task_id: str, project_id: str, shot_id: str) -> None:
        try:
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            if isinstance(self.planner, ScriptPlanner):
                prompt = self.planner.generate_shot_prompt(
                    project,
                    shot,
                    project.get("assets", []),
                    options=self._provider_options(project, "language"),
                )
            else:
                prompt = ScriptPlanner._fallback_shot_prompt(
                    project, shot, project.get("assets", [])
                )
            updated = self.repository.update_shot(
                project_id,
                shot_id,
                prompt=prompt,
                status=GenerationStatus.NOT_GENERATED,
            )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={"shot_id": shot_id, "prompt": updated["prompt"]},
            )
        except Exception as exc:
            self.repository.update_shot(project_id, shot_id, status=GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def run_shot_video(
        self, task_id: str, project_id: str, shot_id: str, video_url: str | None = None
    ) -> None:
        try:
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            resolved_video_url = video_url or self._generate_video_url(project, shot)
            video = {
                "id": str(task_id),
                "url": resolved_video_url,
                "generated_at": datetime.now(timezone.utc).isoformat(),
                "task_id": task_id,
                "model": project.get("multimodal_model"),
                "prompt": shot.get("prompt", ""),
                "mode": "provider" if resolved_video_url else "local_task_preview",
            }
            self.repository.add_historical_video(project_id, shot_id, video)
            self.repository.update_task_status(
                task_id, GenerationStatus.SUCCEEDED, result={"shot_id": shot_id, **video}
            )
        except Exception as exc:
            self.repository.update_shot(
                project_id,
                shot_id,
                status=GenerationStatus.FAILED,
            )
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def save_model_config(self, config: dict[str, Any]) -> dict[str, str]:
        self.settings[config["kind"]] = config
        if config["kind"] == "language" and isinstance(self.planner, ScriptPlanner):
            self.planner.configure(
                {
                    "api_key": config.get("api_key"),
                    "endpoint": config.get("endpoint"),
                    "model": config.get("model"),
                }
            )
        return {"status": "saved", "kind": config["kind"]}

    def _provider_options(self, project: dict[str, Any], kind: str) -> dict[str, Any]:
        configured = self.settings.get(kind, {})
        if kind == "language":
            model = configured.get("model") or project.get("language_model")
        elif kind == "video":
            model = configured.get("video_model") or project.get("video_model")
        else:
            model = configured.get("model") or project.get("multimodal_model")
        return {
            "api_key": configured.get("api_key") or os.getenv("OPENAI_API_KEY"),
            "endpoint": configured.get("endpoint") or os.getenv("OPENAI_BASE_URL"),
            "model": model,
            "style": project.get("style"),
            "theme": project.get("theme"),
            "ratio": project.get("ratio"),
        }

    def _generate_image_url(self, project: dict[str, Any], asset: dict[str, Any]) -> str:
        options = self._provider_options(project, "multimodal")
        if options.get("api_key"):
            client = OpenAICLient(
                OpenAIClientBaseOptions(
                    api_key=options["api_key"],
                    base_url=options.get("endpoint"),
                    model=options.get("model"),
                )
            )
            result = client.sync_client.images.generate(
                model=options.get("model") or "gpt-image-1",
                prompt=asset.get("prompt", ""),
                size="1024x1024",
                n=1,
            )
            item = result.data[0]
            if getattr(item, "url", None):
                return str(item.url)
            encoded = getattr(item, "b64_json", None)
            if encoded:
                return f"data:image/png;base64,{encoded}"
            raise RuntimeError("图片模型没有返回图片地址")

        # Local mode intentionally returns a visible, deterministic preview so
        # the whole UI workflow can be exercised without a provider account.
        title = escape(str(asset.get("name", "素材")))
        kind = escape(str(asset.get("type", "prop")))
        svg = (
            '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="800" viewBox="0 0 640 800">'
            '<defs><linearGradient id="g" x1="0" x2="1" y1="0" y2="1"><stop stop-color="#eee7dd"/><stop offset="1" stop-color="#c8d7d2"/></linearGradient></defs>'
            '<rect width="640" height="800" rx="32" fill="url(#g)"/><circle cx="320" cy="300" r="110" fill="#ffffff99"/>'
            f'<text x="320" y="520" text-anchor="middle" font-size="38" font-family="sans-serif" fill="#202020">{title}</text>'
            f'<text x="320" y="575" text-anchor="middle" font-size="22" font-family="sans-serif" fill="#666">{kind} · 本地预览</text></svg>'
        )
        return "data:image/svg+xml;base64," + base64.b64encode(svg.encode()).decode()

    def _generate_video_url(
        self, project: dict[str, Any], shot: dict[str, Any]
    ) -> str | None:
        """Call an optional provider-specific video endpoint.

        Video APIs are not standardized by the OpenAI SDK. If
        ``VIDEO_GENERATION_ENDPOINT`` is configured, the adapter sends a
        small OpenAI-compatible JSON request and accepts ``url`` or
        ``video_url`` in the response. Without it, the durable task/history
        flow still works locally and records that no remote URL is available.
        """

        endpoint = os.getenv("VIDEO_GENERATION_ENDPOINT")
        if not endpoint:
            return None
        options = self._provider_options(project, "video")
        request = Request(
            endpoint,
            data=json.dumps(
                {
                    "model": options.get("model"),
                    "prompt": shot.get("prompt", ""),
                    "ratio": project.get("ratio", "9:16"),
                    "duration": 10,
                }
            ).encode(),
            headers={
                "Content-Type": "application/json",
                **(
                    {"Authorization": f"Bearer {options['api_key']}"}
                    if options.get("api_key")
                    else {}
                ),
            },
            method="POST",
        )
        with urlopen(request, timeout=90) as response:  # noqa: S310 - configured endpoint
            payload = json.loads(response.read().decode())
        if isinstance(payload, dict):
            result = payload.get("video_url") or payload.get("url")
            if isinstance(result, str) and result:
                return result
            data = payload.get("data")
            if isinstance(data, list) and data and isinstance(data[0], dict):
                result = data[0].get("video_url") or data[0].get("url")
                if isinstance(result, str) and result:
                    return result
        raise RuntimeError("视频模型没有返回 video_url/url")

task_service = TaskService()
