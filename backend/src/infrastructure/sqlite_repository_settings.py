"""ORM-backed settings, prompt-template, and voice-preset persistence."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import select, update

from ..domain.voice_presets import DEFAULT_VOICE_PRESETS
from .orm_models import AppSetting, PromptTemplate, VoicePreset
from .repository_common import _json_dump, _json_load, utc_now


class DramaRepositorySettingsMixin:
    """Owns configuration assets used by prompt generation and model providers.

    The settings page calls these operations when users save provider/model
    configuration, edit prompt template versions, or choose a voice preset.
    Keeping them in an ORM mixin removes SQL details from the public repository
    facade while preserving the existing dictionary response shapes.
    """

    @staticmethod
    def _template_to_dict(template: PromptTemplate) -> dict[str, Any]:
        return {
            "id": template.id,
            "scope": template.scope,
            "name": template.name,
            "version": template.version,
            "template_text": template.template_text,
            "metadata": _json_load(template.metadata_json, {}),
            "active": bool(template.active),
            "created_at": template.created_at,
            "updated_at": template.updated_at,
        }

    def list_prompt_templates(
        self,
        scope: str = "drama",
        name: str | None = None,
        include_inactive: bool = True,
    ) -> list[dict[str, Any]]:
        """Return prompt versions for the prompt-template management API."""

        statement = select(PromptTemplate).where(PromptTemplate.scope == scope)
        if name:
            statement = statement.where(PromptTemplate.name == name)
        if not include_inactive:
            statement = statement.where(PromptTemplate.active == 1)
        statement = statement.order_by(PromptTemplate.name, PromptTemplate.created_at.desc())
        with self.database.session() as session:
            templates = session.scalars(statement).all()
        return [self._template_to_dict(template) for template in templates]

    def get_active_prompt_template(
        self, scope: str, name: str, version: str | None = None
    ) -> dict[str, Any] | None:
        """Return the active template selected by a prompt-generation task."""

        statement = (
            select(PromptTemplate)
            .where(
                PromptTemplate.scope == scope,
                PromptTemplate.name == name,
                PromptTemplate.active == 1,
            )
            .order_by(PromptTemplate.created_at.desc())
            .limit(1)
        )
        if version:
            statement = statement.where(PromptTemplate.version == version)
        with self.database.session() as session:
            template = session.scalar(statement)
        return self._template_to_dict(template) if template else None

    def create_prompt_template(
        self,
        scope: str,
        name: str,
        version: str,
        template_text: str,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Create and activate a new prompt version while deactivating older versions."""

        timestamp = utc_now()
        template = PromptTemplate(
            id=str(uuid4()),
            scope=scope,
            name=name,
            version=version,
            template_text=template_text,
            metadata_json=_json_dump(metadata or {}),
            active=1,
            created_at=timestamp,
            updated_at=timestamp,
        )
        with self.database.session() as session:
            session.execute(
                update(PromptTemplate)
                .where(PromptTemplate.scope == scope, PromptTemplate.name == name)
                .values(active=0, updated_at=timestamp)
            )
            session.add(template)
            session.flush()
        return self._template_to_dict(template)

    @staticmethod
    def _seed_voice_presets() -> None:
        """Seed data is maintained by the compatibility initializer."""

    def list_voice_presets(self) -> list[dict[str, Any]]:
        """List enabled voice presets for the character editor selector."""

        statement = (
            select(VoicePreset.id, VoicePreset.name, VoicePreset.gender, VoicePreset.prompt, VoicePreset.sort_order)
            .where(VoicePreset.enabled == 1)
            .order_by(VoicePreset.sort_order, VoicePreset.id)
        )
        with self.database.session() as session:
            return [dict(row) for row in session.execute(statement).mappings().all()]

    def get_voice_preset(self, voice_id: str | None) -> dict[str, Any] | None:
        """Load one enabled voice preset when an asset stores a voice id."""

        normalized = str(voice_id or "").strip()
        if not normalized or normalized == "none":
            return None
        statement = (
            select(VoicePreset.id, VoicePreset.name, VoicePreset.gender, VoicePreset.prompt, VoicePreset.sort_order)
            .where(VoicePreset.id == normalized, VoicePreset.enabled == 1)
        )
        with self.database.session() as session:
            row = session.execute(statement).mappings().first()
        return dict(row) if row else None

    def get_settings(self) -> dict[str, Any]:
        """Load provider/storage settings for the configuration page and workers."""

        with self.database.session() as session:
            rows = session.execute(select(AppSetting)).scalars().all()
        return {setting.key: _json_load(setting.value_json, {}) for setting in rows}

    def get_setting(self, key: str, default: Any = None) -> Any:
        """Read one setting without exposing its storage representation."""

        with self.database.session() as session:
            setting = session.get(AppSetting, key)
        return _json_load(setting.value_json, default) if setting else default

    def set_setting(self, key: str, value: Any) -> Any:
        """Persist one setting when the frontend saves model or storage configuration."""

        with self.database.session() as session:
            setting = session.get(AppSetting, key)
            if setting is None:
                setting = AppSetting(key=key, value_json=_json_dump(value), updated_at=utc_now())
                session.add(setting)
            else:
                setting.value_json = _json_dump(value)
                setting.updated_at = utc_now()
        return value

