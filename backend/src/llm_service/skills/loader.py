"""Glob-based runtime skill discovery."""

from __future__ import annotations

import hashlib
import importlib.util
import inspect
from pathlib import Path
from typing import Any, Iterable

from .base import BaseSkill


PROJECT_ROOT = Path(__file__).resolve().parents[4]
DEFAULT_SKILL_DIRECTORIES = ["backend/src/llm_service/skills/*"]


class SkillLoader:
    """Discover and instantiate ``BaseSkill`` subclasses from glob paths."""

    def __init__(
        self,
        skill_directories: Iterable[str] | None = None,
        *,
        project_root: Path | None = None,
    ) -> None:
        self.skill_directories = list(skill_directories or DEFAULT_SKILL_DIRECTORIES)
        self.project_root = project_root or PROJECT_ROOT

    def load(self) -> dict[str, BaseSkill]:
        skills: dict[str, BaseSkill] = {}
        for module_path in self._module_paths():
            for skill_class in self._skill_classes(module_path):
                skill = skill_class()
                if skill.name in skills:
                    raise ValueError(
                        f"Duplicate skill name {skill.name!r}: "
                        f"{type(skills[skill.name]).__module__} and {module_path}"
                    )
                skills[skill.name] = skill
        return skills

    def _module_paths(self) -> list[Path]:
        paths: set[Path] = set()
        for pattern in self.skill_directories:
            pattern_path = Path(pattern)
            if not pattern_path.is_absolute():
                pattern_path = self.project_root / pattern_path

            for match in pattern_path.parent.glob(pattern_path.name):
                if match.is_dir():
                    paths.update(match.glob("*.py"))
                elif match.suffix == ".py":
                    paths.add(match)

        return sorted(
            path
            for path in paths
            if path.name not in {"__init__.py", "base.py", "loader.py"}
            and not path.name.startswith("_")
            and path.is_file()
        )

    @staticmethod
    def _skill_classes(module_path: Path) -> list[type[BaseSkill]]:
        module_name = "runtime_skill_" + hashlib.sha1(
            str(module_path).encode("utf-8"), usedforsecurity=False
        ).hexdigest()
        spec = importlib.util.spec_from_file_location(module_name, module_path)
        if spec is None or spec.loader is None:
            raise ImportError(f"Unable to load skill module: {module_path}")

        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return [
            candidate
            for _, candidate in inspect.getmembers(module, inspect.isclass)
            if issubclass(candidate, BaseSkill)
            and candidate is not BaseSkill
            and candidate.__module__ == module.__name__
        ]
