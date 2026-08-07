import logging
import re
from typing import Any

from ..domain.models import GenerationStatus


logger = logging.getLogger(__name__)


class TaskServiceQualityWorkerMixin:
    """Run automatic shot-prompt quality checks after prompt generation.

    The durable drama worker calls this slice after a shot prompt is generated.
    It validates rich references, camera structure, and project constraints, then
    persists a refresh-safe quality result for the shot editor to display.
    """

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
                project_id, shot_id, structured=structured, quality=quality,
                quality_status=quality["status"], quality_issues=issues,
            )
            self.repository.update_task_status(task_id, GenerationStatus.SUCCEEDED, result=quality)
        except Exception as exc:
            failure_issue = {"code": "QUALITY_TASK_FAILED", "severity": "error", "message": str(exc)}
            try:
                self.repository.update_shot(
                    project_id, shot_id,
                    quality={"status": "检查失败", "score": 0, "issues": [failure_issue]},
                    quality_status="检查失败", quality_issues=[failure_issue],
                )
            except Exception:
                logger.exception("Could not persist shot quality failure: %s", shot_id)
            self.repository.update_task_status(task_id, GenerationStatus.FAILED, error_message=str(exc))
