"""Restore decorators lost when AST-splitting methods out of their classes."""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STATIC_METHODS = {
    "backend/src/llm_service/planner_prompt_mixin.py": {
        "rich_prompt_to_text", "_fallback_shot_prompt_rich", "_select_shot_reference_assets",
        "_reference_node", "_append_reference_section", "_normalize_rich_prompt", "_is_structured_shot_prompt",
    },
    "backend/src/llm_service/planner_decomposition_mixin.py": {
        "_decomposition_prompt", "_split_script_into_segments", "_fallback_plan", "_normalize_character_name",
        "_fallback_asset_catalog", "_is_full_script_like", "_clean_script", "_meaningful_asset_name",
    },
    "backend/src/llm_service/planner_repair_mixin.py": {"_repair_asset_catalog", "_normalize_plan"},
    "backend/src/llm_service/planner_utils_mixin.py": {
        "_contains_any", "_unique_specs", "_character_prompt", "_character_story_context",
        "_character_personality", "_scene_prompt", "_parse_json",
    },
    "backend/src/application/task_service_asset_mixin.py": {
        "_placeholder_prompt", "_normalize_placeholder_placements", "_render_placeholder_layout", "_read_media_bytes",
    },
    "backend/src/application/task_service_project_mixin.py": {
        "_collect_media_urls", "_flatten_shots", "_missing_video_references",
    },
    "backend/src/application/task_service_provider_mixin.py": {
        "_ark_endpoint", "_asset_generation_prompt", "_persist_media_url", "_mask_secret",
        "_is_ark_image_provider", "_is_ark_video_provider",
    },
    "backend/src/application/task_service_prompt_mixin.py": {"_structured_from_prompt"},
}

for relative, names in STATIC_METHODS.items():
    path = ROOT / relative
    source = path.read_text()
    tree = ast.parse(source)
    lines = source.splitlines()
    insertions = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.FunctionDef) or node.name not in names:
            continue
        if any(isinstance(item, ast.Name) and item.id == "staticmethod" for item in node.decorator_list):
            continue
        insertions.append(node.lineno - 1)
    for line_number in sorted(insertions, reverse=True):
        lines.insert(line_number, "    @staticmethod")
    path.write_text("\n".join(lines).rstrip() + "\n")
