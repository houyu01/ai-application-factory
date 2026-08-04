"""Short-drama repository initialization and built-in ORM seed data."""

from __future__ import annotations

import json
import os
from pathlib import Path

from ..domain.voice_presets import DEFAULT_VOICE_PRESETS
from .database import Database
from .legacy_sqlite_migrations import migrate_legacy_drama_database
from .orm_models import PromptTemplate, VoicePreset
from .repository_common import _json_dump, utc_now


class DramaRepositorySetupMixin:
    """Initialize the drama repository and seed selectable prompts and voices.

    This class runs during application startup, after SQLAlchemy creates the
    schema. Its only SQL exception is the isolated legacy migration module,
    which upgrades databases created before the ORM model set existed.
    """

    def __init__(self, database_path: str | Path | None = None) -> None:
        default_path = Path(__file__).resolve().parents[2] / "data" / "ai_application_factory.db"
        configured_path = database_path or os.getenv("DATABASE_PATH") or default_path
        self.database_path = Path(configured_path)
        if str(self.database_path) != ":memory:":
            self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self.database = Database(self.database_path)
        migrate_legacy_drama_database(self.database_path)
        self._seed_prompt_templates()
        self._seed_voice_presets()

    def _seed_voice_presets(self) -> None:
        """Insert the built-in voice descriptions used by character settings."""
        timestamp = utc_now()
        with self.database.session() as session:
            for index, preset in enumerate(DEFAULT_VOICE_PRESETS):
                if session.get(VoicePreset, preset["id"]):
                    continue
                session.add(VoicePreset(
                    id=preset["id"], name=preset["name"], gender=preset.get("gender", ""),
                    prompt=preset.get("prompt", ""), sort_order=index, enabled=1,
                    created_at=timestamp, updated_at=timestamp,
                ))

    def _seed_prompt_templates(self) -> None:
        """Register bundled editable shot and quality templates on first startup."""
        template_dir = Path(__file__).resolve().parents[1] / "llm_service" / "templates" / "drama"
        timestamp = utc_now()
        for filename in ("shot_prompt_v1.json", "shot_prompt_v2.json", "shot_quality_v1.json"):
            path = template_dir / filename
            try:
                payload = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                continue
            template_id = f"{payload.get('scope', 'drama')}:{payload.get('name', filename)}:{payload.get('version', 'v1')}"
            with self.database.session() as session:
                if session.get(PromptTemplate, template_id):
                    continue
                session.add(PromptTemplate(
                    id=template_id, scope=str(payload.get("scope") or "drama"),
                    name=str(payload.get("name") or filename), version=str(payload.get("version") or "v1"),
                    template_text=str(payload.get("template") or ""),
                    metadata_json=_json_dump(payload.get("metadata") or {}), active=1,
                    created_at=timestamp, updated_at=timestamp,
                ))
