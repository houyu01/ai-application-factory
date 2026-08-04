from datetime import datetime, timedelta, timezone
from io import BytesIO
import json
import logging
import os
import re
from urllib.request import Request, urlopen
from urllib.parse import urlparse
from typing import Any

from PIL import Image, ImageDraw, ImageOps

from ..domain.models import GenerationStatus, ProjectCreate
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner
from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions


logger = logging.getLogger(__name__)


def utc_now_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()

class TaskServiceProjectMixin:
    """Behavior slice of TaskService."""

    @staticmethod
    def _collect_media_urls(value: Any) -> set[str]:
        """Collect media URLs from asset/shot/history JSON without overmatching."""

        urls: set[str] = set()

        def visit(node: Any) -> None:
            if isinstance(node, dict):
                for key, child in node.items():
                    if key in {"image_url", "video_url", "url"} and isinstance(child, str):
                        if child.strip():
                            urls.add(child.strip())
                    elif isinstance(child, (dict, list)):
                        visit(child)
            elif isinstance(node, list):
                for child in node:
                    visit(child)

        visit(value)
        return urls

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
            episodes = ScriptPlanner._repair_shot_segments(
                plan.get("episodes", []),
                project["script"],
                self._provider_options(project, "language"),
            )
            assets = plan.get("assets", [])
            shots = self._flatten_shots(episodes, assets, project)
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
    def _missing_video_references(
        project: dict[str, Any], shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> list[str]:
        """Return referenced assets that cannot be passed to a video model."""

        reference_ids: list[str] = []
        prompt_rich = shot.get("prompt_rich") or []
        if isinstance(prompt_rich, list):
            reference_ids.extend(
                str(node.get("asset_id"))
                for node in prompt_rich
                if isinstance(node, dict)
                and node.get("type") == "reference"
                and node.get("asset_id")
            )
        if not reference_ids:
            reference_ids.extend(
                str(asset_id)
                for asset_id in (shot.get("reference_asset_ids") or [])
                if asset_id
            )

        assets_by_id = {
            str(asset.get("id")): asset
            for asset in (project.get("assets") or [])
            if asset.get("id")
        }
        missing: list[str] = []
        seen: set[str] = set()
        for asset_id in reference_ids:
            if asset_id in seen:
                continue
            seen.add(asset_id)
            asset = assets_by_id.get(asset_id)
            if asset is None:
                missing.append(f"{asset_id}（素材不存在）")
            elif not str(asset.get("image_url") or "").strip():
                missing.append(f"{asset.get('name') or asset_id}（图片未生成或未上传）")
            elif media_store.provider_reference_url(
                asset.get("image_url"), public_media_base_url
            ) is None:
                missing.append(f"{asset.get('name') or asset_id}（本地图片无法调用大模型）")
        return missing

    def _configured_project_model_selection(self, payload: ProjectCreate) -> ProjectCreate:
        """Keep new projects on models that are currently selectable in Settings.

        The creation form calls this after its settings request has resolved.
        It also protects against a stale browser tab submitting the former
        hard-coded defaults after an administrator has changed provider models.
        """

        fields = {
            "language": "language_model",
            "multimodal": "multimodal_model",
            "video": "video_model",
        }
        updates: dict[str, str] = {}
        for kind, field in fields.items():
            configured = self.settings.get(kind)
            if not isinstance(configured, dict):
                continue
            models = [
                str(item).strip()
                for item in configured.get("models", [])
                if str(item).strip()
            ]
            default_model = str(configured.get("model") or "").strip()
            if default_model and default_model not in models:
                models.insert(0, default_model)
            if not models:
                continue
            selected = str(getattr(payload, field) or "").strip()
            if selected not in models:
                updates[field] = default_model if default_model in models else models[0]
        return payload.model_copy(update=updates) if updates else payload

    def create_project(self, payload: ProjectCreate) -> dict[str, Any]:
        """Persist the empty drama first, using the configured project models."""
        payload = self._configured_project_model_selection(payload)
        project, task = self.repository.create_drama_with_task(payload)
        self.repository.set_drama_status(project["id"], GenerationStatus.GENERATING)
        task = self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)
        project["status"] = GenerationStatus.GENERATING.value
        project["task_id"] = task["id"]
        project["task"] = task
        return project

    @staticmethod
    def _flatten_shots(
        episodes: list[dict[str, Any]], assets: list[dict[str, Any]], project: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Create the first editable rich-prompt draft for every decomposed shot.

        Script decomposition calls this before assets have images.  The draft
        still references the persisted character, scene, and prop records so
        users can review the intended visual dependencies before generating
        any images.  Asset identifiers are scoped by the repository during
        the same decomposition transaction.
        """

        shots: list[dict[str, Any]] = []
        for episode_index, episode in enumerate(episodes, start=1):
            episode_name = episode.get("name", "第1集")
            for index, shot in enumerate(episode.get("shots", []), start=1):
                original_text = shot.get(
                    "original_text", shot.get("script", str(project.get("script") or "")[:120])
                )
                draft = {
                    **shot,
                    "episode_index": episode_index,
                    "episode_name": episode_name,
                    "shot_index": index,
                    "original_text": original_text,
                    "duration_seconds": shot.get("duration_seconds", shot.get("duration", 10)),
                }
                prompt_rich = ScriptPlanner._fallback_shot_prompt_rich(project, draft, assets)
                shots.append(
                    {
                        **draft,
                        "prompt": ScriptPlanner.rich_prompt_to_text(prompt_rich),
                        "prompt_rich": prompt_rich,
                        "reference_asset_ids": [
                            str(node.get("asset_id"))
                            for node in prompt_rich
                            if node.get("type") == "reference" and node.get("asset_id")
                        ],
                        "status": GenerationStatus.NOT_GENERATED.value,
                        "historical_videos": [],
                    }
                )
        return shots

    def get_project(self, project_id: str) -> dict[str, Any]:
        project = self.repository.get_drama(project_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        return project

    def create_shot(self, project_id: str, after_shot_id: str, **values: Any) -> dict[str, Any]:
        """Create an editable blank shot from the shot-list plus button."""
        self.get_project(project_id)
        return self.repository.create_shot_after(project_id, after_shot_id, **values)

    def delete_shot(self, project_id: str, shot_id: str) -> dict[str, Any]:
        """Delete one shot and clean media belonging only to that shot."""
        project = self.get_project(project_id)
        shots = project.get("shots", [])
        current_index = next((index for index, shot in enumerate(shots) if shot.get("id") == shot_id), -1)
        if current_index < 0:
            raise KeyError(f"Shot not found: {shot_id}")
        next_shot_id = next((shot.get("id") for shot in shots[current_index + 1:] if shot.get("id")), None)
        if next_shot_id is None and current_index:
            next_shot_id = shots[current_index - 1].get("id")
        result = self.repository.delete_shot(project_id, shot_id)
        cancel_errors: list[str] = []
        for provider_task_id in result.pop("provider_task_ids", []):
            try:
                self._cancel_remote_video_task(project, provider_task_id)
            except Exception as exc:
                cancel_errors.append(str(exc))
                logger.warning("Failed to cancel remote shot video task %s: %s", provider_task_id, exc)
        cleanup_errors: list[str] = []
        for url in result.pop("media_urls", []):
            try:
                media_store.delete_url(url)
            except Exception as exc:
                cleanup_errors.append(str(exc))
                logger.warning("Failed to delete shot media %s: %s", url, exc)
        result["next_shot_id"] = next_shot_id
        if cleanup_errors:
            result["media_cleanup_errors"] = cleanup_errors
        if cancel_errors:
            result["provider_cancel_errors"] = cancel_errors
        return result

    def _cancel_remote_video_task(self, project: dict[str, Any], provider_task_id: str) -> None:
        """Cancel an already-submitted Ark video request before deleting its shot."""
        options = self._provider_options(project, "video")
        if not options.get("api_key") or not self._is_ark_video_provider(options):
            return
        if not options.get("create_url") or not options.get("query_url"):
            return
        ArkClient(
            api_key=str(options["api_key"]),
            base_url=self._ark_endpoint(options),
            model=str(options.get("model") or "doubao-seedance-2.0"),
            create_url=str(options["create_url"]),
            query_url=str(options["query_url"]),
        ).cancel_video_task(provider_task_id)

    def enqueue(
        self, kind: str, project_id: str, resource_id: str,
        public_media_base_url: str | None = None,
    ) -> dict[str, Any]:
        project = self.repository.get_drama(project_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        if kind == "asset_image" and self.repository.get_asset(project_id, resource_id) is None:
            raise KeyError(f"Asset not found: {resource_id}")
        if kind in {"shot_prompt", "shot_video", "shot_quality"} and self.repository.get_shot(project_id, resource_id) is None:
            raise KeyError(f"Shot not found: {resource_id}")
        active_task = self.repository.get_active_task(project_id, kind, resource_id)
        if active_task is not None:
            return {**active_task, "_reused": True}
        shot_version = None
        if kind == "shot_video":
            shot = self.repository.get_shot(project_id, resource_id)
            missing_references = self._missing_video_references(
                project, shot or {}, public_media_base_url
            )
            if missing_references:
                local_references = [
                    item.split("（", 1)[0] for item in missing_references
                    if "本地图片无法调用大模型" in item
                ]
                if local_references:
                    raise ValueError(
                        "本地生成的图片无法调用大模型，请配置火山引擎/阿里云/"
                        "腾讯云的对象存储地址，或自定义图片上传与访问地址。涉及素材："
                        + "、".join(local_references)
                    )
                raise ValueError(
                    "生成视频前必须先生成或上传引用素材图片："
                    + "、".join(missing_references)
                )
            shot_version = self.repository.create_shot_version(
                project_id,
                resource_id,
                prompt=str(shot.get("prompt") or "") if shot else "",
                prompt_rich=shot.get("prompt_rich") if shot else [],
                structured=shot.get("structured") if shot else {},
                quality=shot.get("quality") if shot else {},
                status=GenerationStatus.GENERATING,
            )
        input_snapshot = {
            "project_id": project_id,
            "resource_id": resource_id,
            "type": kind,
        }
        if kind == "shot_video" and public_media_base_url:
            input_snapshot["public_media_base_url"] = public_media_base_url
        if kind == "shot_prompt":
            shot = self.repository.get_shot(project_id, resource_id)
            input_snapshot["prompt_template_version"] = str(
                (shot or {}).get("prompt_template_version") or "v1"
            )
        if shot_version is not None:
            input_snapshot["version_id"] = shot_version["id"]
        task = self.repository.create_task(
            project_id,
            kind,
            resource_id,
            input_snapshot=input_snapshot,
        )
        if shot_version is not None:
            self.repository.update_shot_version(
                shot_version["id"], task_id=task["id"]
            )
        if kind == "asset_image":
            self.repository.update_asset_status(resource_id, GenerationStatus.GENERATING)
        if kind in {"shot_prompt", "shot_video"}:
            self.repository.update_shot(
                project_id,
                resource_id,
                status=GenerationStatus.GENERATING,
            )
        return {
            **self.repository.update_task_status(task["id"], GenerationStatus.GENERATING),
            "_reused": False,
        }

    def list_projects(self) -> list[dict[str, Any]]:
        return self.repository.list_dramas()

    def delete_project(self, project_id: str) -> dict[str, Any]:
        """Delete a project graph and clean up media owned by that project.

        The repository deletion is deliberately completed before media cleanup:
        a failed object-store request must not leave a project visible while
        still retaining references to orphaned assets.  Unknown URLs are
        ignored by ``MediaStore`` so externally supplied media is never deleted.
        """

        project = self.get_project(project_id)
        media_urls = self._collect_media_urls(project)
        self.repository.delete_drama(project_id)

        media_deleted = 0
        cleanup_errors: list[str] = []
        for url in media_urls:
            try:
                if media_store.delete_url(url):
                    media_deleted += 1
            except Exception as exc:  # cloud cleanup must not undo DB deletion
                cleanup_errors.append(str(exc))
                logger.warning("Failed to delete project media %s: %s", url, exc)

        result: dict[str, Any] = {
            "status": "deleted",
            "id": project_id,
            "media_deleted": media_deleted,
        }
        if cleanup_errors:
            result["media_cleanup_errors"] = cleanup_errors
        return result
