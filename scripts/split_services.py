"""Split the two large orchestration classes into behavior-focused mixins."""

from __future__ import annotations

import ast
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def source_parts(path: Path, class_name: str) -> tuple[str, ast.ClassDef, list[str]]:
    source = path.read_text()
    tree = ast.parse(source)
    node = next(item for item in tree.body if isinstance(item, ast.ClassDef) and item.name == class_name)
    return source, node, source.splitlines()


def method_text(lines: list[str], node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    return "\n".join(lines[node.lineno - 1 : node.end_lineno])


def class_header(lines: list[str], node: ast.ClassDef, base: str) -> str:
    prefix = "\n".join(lines[node.lineno - 1 : node.lineno])
    return re.sub(rf"class {node.name}(?:\([^)]*\))?:", f"class {node.name}({base}):", prefix)


def write_mixins(
    path: Path,
    class_name: str,
    mixins: dict[str, set[str]],
    facade_methods: set[str],
    replacements: dict[str, str] | None = None,
) -> None:
    source, node, lines = source_parts(path, class_name)
    header = "\n".join(lines[: node.lineno - 1]).rstrip() + "\n\n"
    methods = {item.name: item for item in node.body if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))}
    for mixin_name, names in mixins.items():
        body = [class_header(lines, node, mixin_name), f'    """Behavior slice of {class_name}."""', ""]
        for name in names:
            method = methods[name]
            text = method_text(lines, method)
            text = "\n".join(line[4:] if line.startswith("    ") else line for line in text.splitlines())
            text = "\n".join(f"    {line}" if line else "" for line in text.splitlines())
            body.append(text)
            body.append("")
        content = header + "\n".join(body).rstrip() + "\n"
        if replacements:
            for old, new in replacements.items():
                content = content.replace(old, new)
        if class_name == "ScriptPlanner":
            content = content.replace(
                "from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions\n",
                "from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions\n\n\ndef _script_planner():\n    from .planner import ScriptPlanner\n    return ScriptPlanner\n",
            ).replace("ScriptPlanner.", "_script_planner().")
        path.parent.joinpath(mixin_name.lower() + ".py").write_text(content)

    facade_body = [class_header(lines, node, ", ".join(mixins)), "    " + '"""Public compatibility facade for the split orchestration service."""', ""]
    for item in node.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and item.name not in facade_methods:
            continue
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            text = method_text(lines, item)
            facade_body.extend(text.splitlines())
            facade_body.append("")
        elif isinstance(item, (ast.Expr, ast.Assign, ast.AnnAssign)):
            text = method_text(lines, item) if hasattr(item, "lineno") else ""
            facade_body.extend(text.splitlines())
            facade_body.append("")
    facade_header = header
    if class_name == "TaskService":
        imports = (
            "from .task_service_project_mixin import TaskServiceProjectMixin\n"
            "from .task_service_asset_mixin import TaskServiceAssetMixin\n"
            "from .task_service_worker_mixin import TaskServiceWorkerMixin\n"
            "from .task_service_provider_mixin import TaskServiceProviderMixin\n\n"
        )
        facade_header += imports
    else:
        imports = (
            "from .planner_prompt_mixin import ScriptPlannerPromptMixin\n"
            "from .planner_decomposition_mixin import ScriptPlannerDecompositionMixin\n"
            "from .planner_utils_mixin import ScriptPlannerUtilsMixin\n\n"
        )
        facade_header += imports
    path.write_text(facade_header + "\n".join(facade_body).rstrip() + "\n")


write_mixins(
    ROOT / "backend/src/application/task_service.py",
    "TaskService",
    {
        "TaskServiceProjectMixin": {"create_project", "list_projects", "get_project", "delete_project", "_collect_media_urls", "decompose_project", "_flatten_shots", "_missing_video_references", "enqueue"},
        "TaskServiceAssetMixin": {"_normalize_placeholder_placements", "enqueue_placeholder_image", "_placeholder_prompt", "_read_media_bytes", "_render_placeholder_layout", "run_placeholder_image", "enqueue_asset_variant_image", "run_asset_variant_image"},
        "TaskServiceWorkerMixin": {"get_task", "resume_task", "advance_shot_video_task", "_update_shot_version_progress", "_fail_shot_video_task", "run_asset_image", "run_shot_prompt", "_structured_from_prompt", "run_shot_quality", "run_shot_video"},
        "TaskServiceProviderMixin": {"_ark_endpoint", "save_model_config", "_public_model_config", "_mask_secret", "get_model_configs", "save_storage_config", "get_storage_config", "_provider_options", "_asset_generation_prompt", "_assets_with_voice_details", "_generate_image_url", "_generate_video_url", "_persist_media_url", "_persist_provider_result", "_video_generation_prompt", "_video_reference_images", "_is_ark_image_provider", "_is_ark_video_provider"},
    },
    {"__init__"},
)

write_mixins(
    ROOT / "backend/src/llm_service/planner.py",
    "ScriptPlanner",
    {
        "ScriptPlannerPromptMixin": {"generate_shot_prompt", "generate_shot_prompt_rich", "rich_prompt_to_text", "_fallback_shot_prompt_rich", "_select_shot_reference_assets", "_reference_node", "_append_reference_section", "_normalize_rich_prompt", "_is_structured_shot_prompt"},
        "ScriptPlannerDecompositionMixin": {"_agent", "_decomposition_prompt", "_fallback_plan", "_fallback_asset_catalog", "_normalize_plan", "_repair_asset_catalog", "_meaningful_asset_name", "_normalize_character_name", "_clean_script", "_split_script_into_segments", "_is_full_script_like", "_repair_shot_segments", "_fallback_shot_prompt"},
        "ScriptPlannerUtilsMixin": {"_contains_any", "_unique_specs", "_character_prompt", "_character_story_context", "_character_personality", "_scene_prompt", "_parse_json"},
    },
    {"__init__", "configure", "plan"},
)
