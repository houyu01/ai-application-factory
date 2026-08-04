"""Structured-field extraction for the shot rich-prompt workflow."""

from __future__ import annotations

import re
from typing import Any

from ..llm_service.planner import ScriptPlanner


class TaskServicePromptMixin:
    """Translate editor rich-text nodes into durable provider-facing fields."""

    @staticmethod
    def _structured_from_prompt(
        project: dict[str, Any],
        shot: dict[str, Any],
        prompt_rich: list[dict[str, Any]],
    ) -> dict[str, Any]:
        """Build camera, voice, references, and constraint fields for one shot."""

        text = ScriptPlanner.rich_prompt_to_text(prompt_rich)
        references = [
            {
                "asset_id": node.get("asset_id"),
                "asset_type": node.get("asset_type"),
                "label": node.get("label"),
            }
            for node in prompt_rich
            if isinstance(node, dict) and node.get("type") == "reference"
        ]
        camera_shots = []
        for match in re.finditer(
            r"【镜头(\d+)\s*\|\s*时长(\d+)s\s*\|\s*时间：([^】]+)】([^【]*)", text
        ):
            camera_shots.append(
                {
                    "index": int(match.group(1)),
                    "duration_seconds": int(match.group(2)),
                    "time": match.group(3).strip(),
                    "description": match.group(4).strip(),
                }
            )
        voice_blocks = []
        voice_pattern = r"【配音：([^｜】]+).*?VoiceID：([^｜】]+).*?状态：([^｜】]+).*?情绪：([^｜】]+).*?台词：([^】]*)】"
        for match in re.finditer(voice_pattern, text):
            voice_blocks.append(
                {
                    "speaker": match.group(1).strip(),
                    "voice_id": match.group(2).strip(),
                    "state": match.group(3).strip(),
                    "emotion": match.group(4).strip(),
                    "dialogue": match.group(5).strip(),
                }
            )
        grouped_references = {
            asset_type: [item for item in references if item["asset_type"] == asset_type]
            for asset_type in ("scene", "character", "prop", "placeholder")
        }
        camera_duration = sum(item["duration_seconds"] for item in camera_shots)
        return {
            "scene_reference_ids": [item["asset_id"] for item in grouped_references["scene"]],
            "character_reference_ids": [item["asset_id"] for item in grouped_references["character"]],
            "prop_reference_ids": [item["asset_id"] for item in grouped_references["prop"]],
            "placeholder_reference_ids": [item["asset_id"] for item in grouped_references["placeholder"]],
            "reference_assets": grouped_references,
            "references": references,
            "camera_shots": camera_shots,
            "shot_count": len(camera_shots),
            "duration_seconds": camera_duration or int(shot.get("duration") or 0),
            "voice_blocks": voice_blocks,
            "has_dialogue": any(
                item["dialogue"] not in {"", "（无新增台词）", "(无新增台词)"}
                for item in voice_blocks
            ),
            "sections": {
                key: bool(re.search(rf"(?:^|\n){label}：", text))
                for key, label in {
                    "scene": "场景",
                    "characters": "角色",
                    "style": "风格",
                    "lighting": "光线",
                    "position": "位置",
                }.items()
            },
            "style": project.get("style", "真人风格"),
            "ratio": project.get("ratio", "9:16"),
            "resolution": project.get("resolution", "720p"),
            "constraints": project.get("shot_constraints") or {},
            "prompt_template_version": shot.get("prompt_template_version") or "v1",
            "source_text": shot.get("original_text", ""),
        }
