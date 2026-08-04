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

from ..domain.models import GenerationStatus
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner
from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions


logger = logging.getLogger(__name__)


def utc_now_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()

class TaskServiceWorkerMixin:
    """Behavior slice of TaskService."""

    def run_shot_quality(self, task_id: str, project_id: str, shot_id: str) -> None:
        try:
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            assets_by_id = {str(asset.get("id")): asset for asset in project.get("assets", [])}
            issues: list[dict[str, Any]] = []
            prompt = str(shot.get("prompt") or "")
            prompt_rich = shot.get("prompt_rich") or []
            if not prompt.strip() or not prompt_rich:
                issues.append({"code": "EMPTY_PROMPT", "severity": "error", "message": "分镜富文本提示词为空", "field": "prompt"})
            if prompt.strip() == str(shot.get("original_text") or "").strip():
                issues.append({"code": "UNSPLIT_SOURCE_TEXT", "severity": "error", "message": "提示词仍是整段原始剧本，尚未拆解为分镜结构", "field": "prompt"})
            if not any(isinstance(node, dict) and node.get("type") == "text" and str(node.get("text") or "").strip() for node in prompt_rich):
                issues.append({"code": "NO_TEXT_NODE", "severity": "error", "message": "富文本提示词缺少文字描述", "field": "prompt_rich"})
            if not any(isinstance(node, dict) and node.get("type") == "reference" for node in prompt_rich):
                issues.append({"code": "NO_REFERENCE", "severity": "warning", "message": "提示词尚未引用角色、场景、道具或占位图", "field": "references"})
            for node in prompt_rich:
                if not isinstance(node, dict) or node.get("type") != "reference":
                    continue
                asset = assets_by_id.get(str(node.get("asset_id") or ""))
                if asset is None:
                    issues.append({"code": "MISSING_ASSET", "severity": "error", "message": f"引用素材不存在：{node.get('label', '未命名')}", "field": "references"})
                elif asset.get("status") != GenerationStatus.SUCCEEDED.value:
                    issues.append({"code": "ASSET_NOT_READY", "severity": "error", "message": f"素材尚未生成成功：{asset.get('name', '未命名')}", "field": "references"})
                elif not asset.get("image_url"):
                    issues.append({"code": "MISSING_IMAGE", "severity": "error", "message": f"素材尚未生成图片：{asset.get('name', '未命名')}", "field": "references"})
            structured = shot.get("structured") or self._structured_from_prompt(project, shot, prompt_rich)
            if not structured.get("scene_reference_ids"):
                issues.append({"code": "MISSING_SCENE", "severity": "warning", "message": "分镜没有场景参考图", "field": "scene_reference_ids"})
            if not structured.get("camera_shots"):
                issues.append({"code": "MISSING_CAMERA", "severity": "error", "message": "没有检测到镜头结构", "field": "camera_shots"})
            elif any(not item.get("description") for item in structured["camera_shots"]):
                issues.append({"code": "EMPTY_CAMERA_DESCRIPTION", "severity": "error", "message": "存在没有动作描述的镜头", "field": "camera_shots"})
            sections = structured.get("sections") or {}
            for key, label in (("scene", "场景"), ("style", "风格"), ("lighting", "光线"), ("position", "位置")):
                if not sections.get(key):
                    issues.append({"code": f"MISSING_{key.upper()}", "severity": "warning", "message": f"提示词缺少{label}结构段落", "field": "prompt"})
            if structured.get("shot_count", 0) > 1 and len(structured.get("voice_blocks") or []) not in {0, structured["shot_count"]}:
                issues.append({"code": "VOICE_SHOT_MISMATCH", "severity": "warning", "message": "配音段落数量与镜头数量不一致", "field": "voice_blocks"})
            constraints = project.get("shot_constraints") or {}
            if not constraints.get("subtitles") and re.search(r"字幕", prompt):
                issues.append({"code": "SUBTITLE_CONSTRAINT", "severity": "error", "message": "项目禁止字幕，但提示词包含字幕描述", "field": "prompt"})
            if not constraints.get("background_music") and re.search(r"背景音乐|配乐", prompt):
                issues.append({"code": "MUSIC_CONSTRAINT", "severity": "error", "message": "项目禁止背景音乐，但提示词包含音乐描述", "field": "prompt"})
            if re.search(r"https?://|data:image|asset://|tos://", prompt):
                issues.append({"code": "TECHNICAL_REFERENCE", "severity": "error", "message": "提示词不能包含图片 URL 或技术标识", "field": "prompt"})
            errors = sum(1 for issue in issues if issue["severity"] == "error")
            quality = {
                "status": "通过" if errors == 0 else "需修改",
                "score": max(0, 100 - errors * 25 - sum(5 for issue in issues if issue["severity"] == "warning")),
                "issues": issues,
                "checks": {
                    "references": not any(issue["code"] in {"MISSING_ASSET", "ASSET_NOT_READY", "MISSING_IMAGE"} for issue in issues),
                    "camera": not any(issue["code"] == "MISSING_CAMERA" for issue in issues),
                    "constraints": not any(issue["code"] in {"SUBTITLE_CONSTRAINT", "MUSIC_CONSTRAINT"} for issue in issues),
                },
            }
            self.repository.update_shot(
                project_id,
                shot_id,
                structured=structured,
                quality=quality,
                quality_status=quality["status"],
                quality_issues=issues,
            )
            self.repository.update_task_status(task_id, GenerationStatus.SUCCEEDED, result=quality)
        except Exception as exc:
            try:
                self.repository.update_shot(
                    project_id,
                    shot_id,
                    quality={"status": "检查失败", "score": 0, "issues": [{"code": "QUALITY_TASK_FAILED", "severity": "error", "message": str(exc)}]},
                    quality_status="检查失败",
                    quality_issues=[{"code": "QUALITY_TASK_FAILED", "severity": "error", "message": str(exc)}],
                )
            except Exception:
                logger.exception("Could not persist shot quality failure: %s", shot_id)
            self.repository.update_task_status(task_id, GenerationStatus.FAILED, error_message=str(exc))

    def _fail_shot_video_task(
        self, task: dict[str, Any], project_id: str, shot_id: str, message: str
    ) -> None:
        self.repository.update_shot(project_id, shot_id, status=GenerationStatus.FAILED)
        self.repository.update_task_status(
            task["id"], GenerationStatus.FAILED, error_message=message
        )
        self._update_shot_version_progress(
            task, status=GenerationStatus.FAILED, error_message=message
        )

    def advance_shot_video_task(self, task: dict[str, Any]) -> None:
        """Submit or poll a real Ark video task without blocking a web request."""
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
        if not options.get("api_key"):
            self.repository.update_task_status(
                task["id"], GenerationStatus.FAILED,
                error_message="未配置视频模型 API Key，无法生成视频",
            )
            self.repository.update_shot(project_id, shot_id, status=GenerationStatus.FAILED)
            return

        if not self._is_ark_video_provider(options):
            # Non-Ark adapters may still expose a blocking SDK. They run in
            # the durable worker, so the HTTP request is not held open.
            self.run_shot_video(task["id"], project_id, shot_id)
            return

        if not options.get("create_url") or not options.get("query_url"):
            self._fail_shot_video_task(
                task, project_id, shot_id, "视频模型必须配置创建任务 URL 和查询任务 URL"
            )
            return

        client = ArkClient(
            api_key=str(options["api_key"]),
            base_url=self._ark_endpoint(options),
            model=str(options.get("model") or "doubao-seedance-2.0"),
            create_url=str(options["create_url"]),
            query_url=str(options["query_url"]),
        )
        provider_task_id = str(task.get("provider_task_id") or "").strip()
        if not provider_task_id:
            created = client.create_video_task(
                self._video_generation_prompt(project, shot),
                ratio=str(project.get("ratio") or "9:16"),
                resolution=str(project.get("resolution") or "720p"),
                seconds=int(shot.get("duration_seconds") or shot.get("duration") or 10),
                reference_images=self._video_reference_images(
                    project, shot, public_media_base_url
                ),
            )
            self.repository.update_task_progress(
                task["id"],
                provider_task_id=str(created["provider_task_id"]),
                progress=int(created.get("progress") or 5),
                stage="provider_submitted",
                next_poll_at=utc_now_after(3),
            )
            self._update_shot_version_progress(
                task,
                progress=int(created.get("progress") or 5),
                provider_task_id=str(created["provider_task_id"]),
            )
            return

        result = client.get_video_task(provider_task_id)
        provider_status = ArkClient._read_status(result)
        if provider_status in {"succeeded", "completed", "success", "succeed"}:
            video_url = ArkClient._read_video_url(result)
            if not video_url:
                self._fail_shot_video_task(task, project_id, shot_id, "视频任务完成但没有返回视频地址")
                return
            self.run_shot_video(
                task["id"], project_id, shot_id, self._persist_media_url(video_url, ".mp4")
            )
            return
        if provider_status in {"failed", "canceled", "cancelled", "error"}:
            self._fail_shot_video_task(
                task, project_id, shot_id,
                ArkClient._read_error(result) or f"视频模型任务状态：{provider_status}",
            )
            return

        progress = ArkClient._read_progress(result)
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
                result={
                    "asset_id": asset_id,
                    "image_url": image_url,
                    "prompt": self._asset_generation_prompt(project, asset),
                },
            )
        except Exception as exc:
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
        elif task_type == "asset_image":
            self.run_asset_image(task_id, project_id, resource_id)
        elif task_type == "asset_variant_image":
            self.run_asset_variant_image(
                task_id,
                project_id,
                str(snapshot.get("asset_id") or ""),
                str(snapshot.get("variant_id") or resource_id),
            )
        elif task_type == "shot_prompt":
            self.run_shot_prompt(task_id, project_id, resource_id)
        elif task_type == "shot_quality":
            self.run_shot_quality(task_id, project_id, resource_id)
        elif task_type == "shot_video":
            self.advance_shot_video_task(task)
        elif task_type == "placeholder_image":
            self.run_placeholder_image(task_id, project_id, resource_id)
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
        except Exception as exc:
            self.repository.update_shot(
                project_id,
                shot_id,
                status=GenerationStatus.FAILED,
            )
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
            self._update_shot_version_progress(
                self.repository.get_task(task_id) or {"id": task_id},
                status=GenerationStatus.FAILED,
                error_message=str(exc),
            )

    def run_shot_prompt(self, task_id: str, project_id: str, shot_id: str) -> None:
        try:
            project = self.get_project(project_id)
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is None:
                raise KeyError(f"Shot not found: {shot_id}")
            assets = self._assets_with_voice_details(project.get("assets", []))
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
                    assets,
                    options=prompt_options,
                )
                prompt = ScriptPlanner.rich_prompt_to_text(prompt_rich)
            else:
                prompt = ScriptPlanner._fallback_shot_prompt(
                    project, shot, assets
                )
                prompt_rich = [{"type": "text", "text": prompt}]
            updated = self.repository.update_shot(
                project_id,
                shot_id,
                prompt=prompt,
                prompt_rich=prompt_rich,
                structured=self._structured_from_prompt(project, shot, prompt_rich),
                quality_status="未检查",
                quality_issues=[],
                reference_asset_ids=[
                    str(node.get("asset_id"))
                    for node in prompt_rich
                    if isinstance(node, dict)
                    and node.get("type") == "reference"
                    and node.get("asset_id")
                ],
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
