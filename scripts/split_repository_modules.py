"""Split the legacy repository classes into small behavior-preserving mixins.

This is a one-shot refactoring utility, not a runtime dependency. It extracts
complete methods from the pre-refactor repository files so the public facade
class and every persistence operation keep their existing names and behavior.
"""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT / "backend" / "src" / "infrastructure"


def method_sources(path: Path) -> dict[str, str]:
    source = path.read_text(encoding="utf-8")
    tree = ast.parse(source)
    repository = next(
        node for node in tree.body if isinstance(node, ast.ClassDef)
    )
    lines = source.splitlines()
    methods: dict[str, str] = {}
    for node in repository.body:
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        start = min(
            [node.lineno, *(decorator.lineno for decorator in node.decorator_list)]
        )
        end = node.end_lineno or node.lineno
        methods[node.name] = "\n".join(lines[start - 1 : end])
    return methods


def write_mixin(path: Path, class_name: str, methods: list[str], source: dict[str, str]) -> None:
    body = "\n\n".join(source[name] for name in methods)
    header = (
        '"""Persistence behavior extracted from the public repository facade."""\n\n'
        "from __future__ import annotations\n\n"
        "import json\nimport os\nimport sqlite3\n"
        "from contextlib import contextmanager\n"
        "from datetime import datetime, timedelta, timezone\n"
        "from pathlib import Path\n"
        "from typing import Any, Iterator\n"
        "from uuid import uuid4\n\n"
        "from ..domain.models import GenerationStatus, InteractiveGameCreate, ProjectCreate\n"
        "from ..domain.voice_presets import DEFAULT_VOICE_PRESETS\n"
        "from .repository_common import _json_dump, _json_load, _parse_datetime, utc_now\n\n\n"
    )
    class_header = f'class {class_name}:\n    """Owns the {class_name.removesuffix("Mixin")} persistence slice."""\n\n'
    path.write_text(header + class_header + body + "\n", encoding="utf-8")


def split_drama() -> None:
    source = method_sources(BACKEND / "sqlite_repository.py")
    groups = {
        "sqlite_repository_setup.py": ("DramaRepositorySetupMixin", ["__init__", "_connect", "_initialize", "_seed_voice_presets", "_seed_prompt_templates", "_ensure_optional_columns", "_migrate_legacy_episodes", "_remove_legacy_episode_snapshot"]),
        "sqlite_repository_settings.py": ("DramaRepositorySettingsMixin", ["list_prompt_templates", "get_active_prompt_template", "create_prompt_template", "list_voice_presets", "get_voice_preset", "get_settings", "get_setting", "set_setting"]),
        "sqlite_repository_mapping.py": ("DramaRepositoryMappingMixin", ["_drama_from_row", "_asset_from_row", "_shot_from_row", "_aggregate_episodes", "_task_from_row", "_shot_version_from_row"]),
        "sqlite_repository_projects.py": ("DramaRepositoryProjectMixin", ["create_drama_with_task", "list_dramas", "get_drama", "delete_drama", "update_public_prompt", "update_model_selection", "update_project_parameters", "update_video_public_prompt", "update_asset_public_prompt", "set_drama_status"]),
        "sqlite_repository_decomposition.py": ("DramaRepositoryDecompositionMixin", ["save_decomposition"]),
        "sqlite_repository_tasks.py": ("DramaRepositoryTaskMixin", ["create_task", "get_task", "claim_next_runnable_task", "update_task_progress", "update_task_input_snapshot", "reschedule_task", "get_active_task", "get_active_task_by_snapshot", "update_task_status"]),
        "sqlite_repository_assets.py": ("DramaRepositoryAssetMixin", ["get_asset", "create_asset", "delete_asset", "_asset_variant", "create_asset_variant", "update_asset_variant", "delete_asset_variant", "update_asset_variant_status", "update_asset_status", "find_asset_by_content_hash", "update_asset", "set_asset_image"]),
        "sqlite_repository_shots.py": ("DramaRepositoryShotMixin", ["get_shot", "update_shot", "create_shot_version", "list_shot_versions", "update_shot_version", "add_historical_video"]),
    }
    for filename, (class_name, names) in groups.items():
        write_mixin(BACKEND / filename, class_name, names, source)
    facade = '''"""Short-drama persistence facade."""

from .repository_common import JSON_FIELDS, _json_dump, _json_load, _parse_datetime, utc_now
from .sqlite_repository_assets import DramaRepositoryAssetMixin
from .sqlite_repository_decomposition import DramaRepositoryDecompositionMixin
from .sqlite_repository_mapping import DramaRepositoryMappingMixin
from .sqlite_repository_projects import DramaRepositoryProjectMixin
from .sqlite_repository_settings import DramaRepositorySettingsMixin
from .sqlite_repository_setup import DramaRepositorySetupMixin
from .sqlite_repository_shots import DramaRepositoryShotMixin
from .sqlite_repository_tasks import DramaRepositoryTaskMixin


class SQLiteRepository(
    DramaRepositorySetupMixin,
    DramaRepositorySettingsMixin,
    DramaRepositoryMappingMixin,
    DramaRepositoryProjectMixin,
    DramaRepositoryDecompositionMixin,
    DramaRepositoryTaskMixin,
    DramaRepositoryAssetMixin,
    DramaRepositoryShotMixin,
):
    """Compatibility facade for all short-drama repository operations."""

'''
    (BACKEND / "sqlite_repository.py").write_text(facade, encoding="utf-8")


def split_games() -> None:
    source = method_sources(BACKEND / "interactive_game_repository.py")
    groups = {
        "game_repository_setup.py": ("GameRepositorySetupMixin", ["__init__", "_connect", "_initialize"]),
        "game_repository_mapping.py": ("GameRepositoryMappingMixin", ["_game_from_row", "_asset_from_row", "_node_from_row", "_edge_from_row", "_task_from_row"]),
        "game_repository_graph.py": ("GameRepositoryGraphMixin", ["create_game_with_task", "list_games", "get_game", "delete_game", "update_model_selection", "set_game_status", "save_graph"]),
        "game_repository_tasks.py": ("GameRepositoryTaskMixin", ["create_task", "get_task", "claim_next_runnable_task", "update_task_progress", "get_active_task", "update_task_status"]),
        "game_repository_runtime.py": ("GameRepositoryRuntimeMixin", ["update_node", "add_node_video", "create_edge", "update_edge", "delete_edge", "create_session", "get_session", "choose_session_edge"]),
    }
    for filename, (class_name, names) in groups.items():
        write_mixin(BACKEND / filename, class_name, names, source)
    facade = '''"""Interactive-game persistence facade."""

from .game_repository_graph import GameRepositoryGraphMixin
from .game_repository_mapping import GameRepositoryMappingMixin
from .game_repository_runtime import GameRepositoryRuntimeMixin
from .game_repository_setup import GameRepositorySetupMixin
from .game_repository_tasks import GameRepositoryTaskMixin


class InteractiveGameRepository(
    GameRepositorySetupMixin,
    GameRepositoryMappingMixin,
    GameRepositoryGraphMixin,
    GameRepositoryTaskMixin,
    GameRepositoryRuntimeMixin,
):
    """Compatibility facade for all interactive-game repository operations."""

'''
    (BACKEND / "interactive_game_repository.py").write_text(facade, encoding="utf-8")


def main() -> None:
    common = '''"""Small persistence helpers shared by repository modules."""

import json
from datetime import datetime, timezone
from typing import Any

JSON_FIELDS = {
    "shots_json": "shots",
    "assets_json": "assets",
    "historical_videos_json": "historical_videos",
    "asset_public_prompts_json": "asset_public_prompts",
    "shot_constraints_json": "shot_constraints",
    "result_json": "result",
}

def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()

def _parse_datetime(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None

def _json_load(value: str | None, default: Any) -> Any:
    if not value:
        return default
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return default

def _json_dump(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False)
'''
    (BACKEND / "repository_common.py").write_text(common, encoding="utf-8")
    split_drama()
    split_games()


if __name__ == "__main__":
    main()
