from datetime import datetime, timedelta, timezone
from io import BytesIO
import json
import logging
import os
from urllib.request import Request, urlopen
from urllib.parse import urlparse
from typing import Any

from PIL import Image, ImageDraw, ImageOps

from ..domain.models import GenerationStatus
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner
from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions
from .task_service_quality_worker_mixin import TaskServiceQualityWorkerMixin


logger = logging.getLogger(__name__)

def utc_now_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()

class TaskServiceWorkerMixin(TaskServiceQualityWorkerMixin):
    """Resume durable short-drama generation tasks in the background worker.

    The application task worker calls this slice for video, prompt, image, and
    placeholder jobs. It owns provider progression and synchronized persistence
    of task, shot, asset, and version states across refreshes and restarts.
    """

    def fail_shot_video_task(
        self, task: dict[str, Any], project_id: str, shot_id: str, message: str
    ) -> None:
        if self._video_task_cancelled(str(task["id"])):
            return
        self.repository.update_task_status(
            task["id"], GenerationStatus.FAILED, error_message=message
        )
        self._update_shot_version_progress(
            task, status=GenerationStatus.FAILED, error_message=message
        )
        self.sync_shot_video_status(project_id, shot_id, GenerationStatus.FAILED)

    def _video_task_cancelled(self, task_id: str) -> bool:
        """Avoid letting an in-flight worker overwrite a creator cancellation."""

        saved = self.repository.get_task(task_id)
        return saved is None or saved.get("status") == GenerationStatus.CANCELLED.value

    @staticmethod
    def video_task_failure_message(error: Exception, model: str) -> str:
        """Translate provider failures into actionable project configuration feedback."""
        detail = str(error)
        if "InputImageSensitiveContentDetected" in detail:
            return (
                "Ark 拒绝了本次视频输入图片：首尾帧或参考图可能包含真实人物/隐私信息，"
                "因此任务已停止。请更换为不含真实人物面部或敏感隐私信息的图片后重试；"
                f"服务商原始错误：{detail}"
            )
        if "UnsupportedModel" in detail or "does not support the agent plan feature" in detail:
            return (
                f"当前视频模型“{model or '未命名模型'}”不支持已配置的 Ark 视频生成任务接口（Agent Plan）。"
                "请先在“配置 → 视频模型”中配置兼容模型，再到当前短剧的“全局参数 → 视频模型”"
                f"中选择该模型后重试。服务商原始错误：{detail}"
            )
        return detail

    def advance_shot_video_task(self, task: dict[str, Any]) -> None:
        """Submit or poll a real Ark video task without blocking a web request."""
        if self._video_task_cancelled(str(task["id"])):
            return
        project_id = str(task["drama_id"])
        shot_id = str(task.get("resource_id") or "")
        project = self.get_project(project_id)
        shot = self.repository.get_shot(project_id, shot_id)
        public_media_base_url = str(
            (task.get("input_snapshot") or {}).get("public_media_base_url") or ""
        ) or None
        if shot is None:
            self.repository.update_task_status(
                task["id"], GenerationStatus.FAILED, error_message="分镜不存在"
            )
            return

        options = self._provider_options(project, "video")
        try:
            client = self._video_task_client(options)
        except ValueError as exc:
            self.fail_shot_video_task(
                task, project_id, shot_id, str(exc)
            )
            return
        if client is None:
            # Blocking adapters still run in the durable worker rather than in
            # the web request, preserving the existing OpenAI-compatible path.
            self.run_shot_video(task["id"], project_id, shot_id)
            return
        provider_task_id = str(task.get("provider_task_id") or "").strip()
        if not provider_task_id:
            if self._video_task_cancelled(str(task["id"])):
                return
            try:
                prompt, reference_images = self._video_generation_inputs(
                    project, shot, public_media_base_url,
                    reference_limit=self._wan_reference_image_limit(options),
                )
                created = client.create_video_task(
                    prompt,
                    ratio=str(project.get("ratio") or "9:16"),
                    resolution=str(project.get("resolution") or "720p"),
                    seconds=int(shot.get("duration_seconds") or shot.get("duration") or 10),
                    reference_images=reference_images,
                )
            except Exception as exc:
                message = self.video_task_failure_message(
                    exc, str(options.get("model") or "")
                )
                self.fail_shot_video_task(task, project_id, shot_id, message)
                logger.warning("Video task submission failed: %s", message)
                return
            saved_task = self.repository.update_task_progress(
                task["id"],
                provider_task_id=str(created["provider_task_id"]),
                progress=int(created.get("progress") or 5),
                stage="provider_submitted",
                next_poll_at=utc_now_after(3),
            )
            if saved_task.get("status") == GenerationStatus.CANCELLED.value:
                try:
                    client.cancel_video_task(str(created["provider_task_id"]))
                except Exception as exc:
                    logger.warning("Failed to cancel newly-created Ark task: %s", exc)
                return
            self._update_shot_version_progress(
                task,
                progress=int(created.get("progress") or 5),
                provider_task_id=str(created["provider_task_id"]),
            )
            return

        try:
            result = client.get_video_task(provider_task_id)
        except Exception as exc:
            if "UnsupportedModel" not in str(exc) and "does not support the agent plan feature" not in str(exc):
                raise
            message = self.video_task_failure_message(
                exc, str(options.get("model") or "")
            )
            self.fail_shot_video_task(task, project_id, shot_id, message)
            return
        reader = self._video_response_reader(options)
        provider_status = reader._read_status(result)
        if provider_status in {"succeeded", "completed", "success", "succeed"}:
            video_url = reader._read_video_url(result)
            if not video_url:
                self.fail_shot_video_task(task, project_id, shot_id, "视频任务完成但没有返回视频地址")
                return
            self.run_shot_video(
                task["id"], project_id, shot_id, self._persist_media_url(video_url, ".mp4")
            )
            return
        if provider_status in {"failed", "canceled", "cancelled", "error"}:
            provider_error = reader._read_error(result) or f"视频模型任务状态：{provider_status}"
            self.fail_shot_video_task(
                task, project_id, shot_id,
                self.video_task_failure_message(
                    RuntimeError(provider_error), str(options.get("model") or "")
                ),
            )
            return

        progress = reader._read_progress(result)
        self.repository.update_task_progress(
            task["id"],
            progress=progress,
            stage=f"provider_{provider_status or 'processing'}",
            next_poll_at=utc_now_after(3),
        )
        self._update_shot_version_progress(task, progress=progress)

    def get_task(self, task_id: str) -> dict[str, Any]:
        task = self.repository.get_task(task_id)
        if task is None:
            raise KeyError(f"Task not found: {task_id}")
        return task

    def _update_shot_version_progress(
        self,
        task: dict[str, Any],
        *,
        progress: int | None = None,
        provider_task_id: str | None = None,
        status: GenerationStatus | None = None,
        video_url: str | None = None,
        error_message: str | None = None,
    ) -> None:
        version_id = str((task.get("input_snapshot") or {}).get("version_id") or "")
        if not version_id:
            return
        try:
            self.repository.update_shot_version(
                version_id,
                status=status,
                progress=progress,
                provider_task_id=provider_task_id,
                video_url=video_url,
                error_message=error_message,
            )
        except KeyError:
            logger.warning("Shot version disappeared while task was running: %s", version_id)

    def run_asset_image(self, task_id: str, project_id: str, asset_id: str) -> None:
        try:
            if self._asset_image_task_cancelled(task_id):
                return
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            image_url = self._generate_image_url(project, asset)
            if self._asset_image_task_cancelled(task_id):
                return
            self.repository.update_asset_status(
                asset_id,
                GenerationStatus.SUCCEEDED,
                image_url=image_url,
            )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "asset_id": asset_id,
                    "image_url": image_url,
                    "prompt": self._asset_generation_prompt(project, asset),
                },
            )
        except Exception as exc:
            if self._asset_image_task_cancelled(task_id):
                return
            self.repository.update_asset_status(asset_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
            logger.exception(
                "Asset image generation failed: task=%s project=%s asset=%s",
                task_id,
                project_id,
                asset_id,
            )

    def resume_task(self, task: dict[str, Any]) -> None:
        """Resume one persisted task after a process restart.

        The worker deliberately dispatches by task type instead of keeping
        Python callables in memory. Every input needed to continue a job is
        stored in ``input_snapshot`` or on the project/shot row.
        """
        task_id = str(task["id"])
        project_id = str(task["drama_id"])
        resource_id = str(task.get("resource_id") or "")
        task_type = str(task.get("type") or "")
        snapshot = task.get("input_snapshot") or {}
        if task_type == "script_decomposition":
            self.decompose_project(task_id, project_id)
        elif task_type == "script_expansion":
            self.run_expanded_script_continuation(task_id, project_id)
        elif task_type == "asset_image":
            self.run_asset_image(task_id, project_id, resource_id)
        elif task_type == "asset_variant_image":
            self.run_asset_variant_image(
                task_id,
                project_id,
                str(snapshot.get("asset_id") or ""),
                str(snapshot.get("variant_id") or resource_id),
            )
        elif task_type in {"asset_image_batch", "shot_reference_image_batch"}:
            self.run_asset_image_batch(task)
        elif task_type == "shot_prompt":
            self.run_shot_prompt(task_id, project_id, resource_id)
        elif task_type == "shot_quality":
            self.run_shot_quality(task_id, project_id, resource_id)
        elif task_type == "shot_video":
            self.advance_shot_video_task(task)
        elif task_type == "placeholder_image":
            self.run_placeholder_image(task_id, project_id, resource_id)
        elif task_type == "cover_image":
            self.run_cover_image(task_id, project_id, resource_id)
        else:
            self.repository.update_task_status(
                task_id,
                GenerationStatus.FAILED,
                error_message=f"未知的任务类型：{task_type}",
            )

    def run_shot_video(
        self, task_id: str, project_id: str, shot_id: str, video_url: str | None = None
    ) -> None:
        try:
            if self._video_task_cancelled(task_id):
                return
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            task = self.repository.get_task(task_id) or {}
            public_media_base_url = str(
                (task.get("input_snapshot") or {}).get("public_media_base_url") or ""
            ) or None
            resolved_video_url = video_url or self._generate_video_url(
                project, shot, public_media_base_url
            )
            if self._video_task_cancelled(task_id):
                return
            video = {
                "id": str(task_id),
                "url": resolved_video_url,
                "generated_at": datetime.now(timezone.utc).isoformat(),
                "task_id": task_id,
                "model": self._provider_options(project, "video").get("model"),
                "prompt": self._video_generation_prompt(project, shot),
                "mode": "provider" if resolved_video_url else "local_task_preview",
            }
            self.repository.add_historical_video(project_id, shot_id, video)
            self.repository.update_task_status(
                task_id, GenerationStatus.SUCCEEDED, result={"shot_id": shot_id, **video}
            )
            self._update_shot_version_progress(
                self.repository.get_task(task_id) or {"id": task_id},
                status=GenerationStatus.SUCCEEDED,
                progress=100,
                video_url=resolved_video_url,
            )
            self.sync_shot_video_status(project_id, shot_id, GenerationStatus.SUCCEEDED)
        except Exception as exc:
            if self._video_task_cancelled(task_id):
                return
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
            self._update_shot_version_progress(
                self.repository.get_task(task_id) or {"id": task_id},
                status=GenerationStatus.FAILED,
                error_message=str(exc),
            )
            self.sync_shot_video_status(project_id, shot_id, GenerationStatus.FAILED)

    def run_shot_prompt(self, task_id: str, project_id: str, shot_id: str) -> None:
        try:
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            assets = self._assets_with_voice_details(project.get("assets", []))
            reference_assets = ScriptPlanner.select_ready_shot_reference_assets(
                shot, assets
            )
            task = self.repository.get_task(task_id) or {}
            task_snapshot = task.get("input_snapshot") or {}
            template_version = str(
                task_snapshot.get("prompt_template_version")
                or shot.get("prompt_template_version")
                or "v1"
            )
            template = self.repository.get_active_prompt_template(
                "drama", "shot_prompt", version=template_version
            )
            if template is None:
                template = self.repository.get_active_prompt_template("drama", "shot_prompt")
            prompt_options = {
                **self._provider_options(project, "language"),
                "prompt_template": template.get("template_text") if template else "",
                "prompt_template_version": template.get("version", "v1") if template else "v1",
            }
            if isinstance(self.planner, ScriptPlanner):
                prompt_rich = self.planner.generate_shot_prompt_rich(
                    project,
                    shot,
                    reference_assets,
                    options=prompt_options,
                )
                prompt = ScriptPlanner.rich_prompt_to_text(prompt_rich)
            else:
                prompt = ScriptPlanner._fallback_shot_prompt(
                    project, shot, reference_assets
                )
                prompt_rich = [{"type": "text", "text": prompt}]
            prompt_rich = ScriptPlanner.ensure_shot_references(
                prompt_rich, reference_assets
            )
            prompt = ScriptPlanner.rich_prompt_to_text(prompt_rich)
            updated = self.repository.update_shot(
                project_id,
                shot_id,
                prompt=prompt,
                prompt_rich=prompt_rich,
                structured=self._structured_from_prompt(project, shot, prompt_rich),
                quality_status="未检查",
                quality_issues=[],
                reference_asset_ids=list(dict.fromkeys(
                    str(node.get("asset_id"))
                    for node in prompt_rich
                    if isinstance(node, dict)
                    and node.get("type") == "reference"
                    and node.get("asset_id")
                )),
                prompt_template_id=template.get("id") if template else None,
                prompt_template_version=template.get("version", "v1") if template else "v1",
                status=GenerationStatus.NOT_GENERATED,
            )
            quality_task = None
            try:
                quality_task = self.enqueue("shot_quality", project_id, shot_id)
                self.repository.update_shot(
                    project_id, shot_id, quality_status="检查中", quality_issues=[]
                )
            except Exception as quality_exc:
                logger.exception("Could not enqueue automatic shot quality check: %s", quality_exc)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "shot_id": shot_id,
                    "prompt": updated["prompt"],
                    "prompt_rich": updated["prompt_rich"],
                    "quality_task_id": quality_task.get("id") if quality_task else None,
                },
            )
        except Exception as exc:
            try:
                self.repository.update_shot(project_id, shot_id, status=GenerationStatus.FAILED)
            except KeyError:
                logger.warning("Shot disappeared while prompt task failed: %s", shot_id)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
