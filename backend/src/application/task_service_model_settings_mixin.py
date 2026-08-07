"""Persist public model settings and their independent generation queues."""

from __future__ import annotations

import os
from typing import Any

from ..llm_service.planner import ScriptPlanner
from .model_provider_profiles import normalized_provider, provider_defaults


class TaskServiceModelSettingsMixin:
    """Serve the settings page without exposing provider secrets.

    The settings page calls this mixin to read and save model selections. It
    owns each model queue's concurrency setting so the durable worker can use
    the same independent limits after a process restart.
    """

    DEFAULT_GENERATION_CONCURRENCY = 2
    MIN_GENERATION_CONCURRENCY = 1
    MAX_GENERATION_CONCURRENCY = 8

    @classmethod
    def _validate_generation_concurrency(cls, value: Any) -> int:
        """Return one bounded concurrent-generation count from a settings value."""

        try:
            concurrency = int(value)
        except (TypeError, ValueError) as exc:
            raise ValueError("生成并发数必须是 1 到 8 的整数") from exc
        if not cls.MIN_GENERATION_CONCURRENCY <= concurrency <= cls.MAX_GENERATION_CONCURRENCY:
            raise ValueError("生成并发数必须在 1 到 8 之间")
        return concurrency

    def _configured_generation_concurrency(self, configured: dict[str, Any]) -> int:
        """Read the persisted count while preserving a safe default for older settings."""

        value = configured.get("generation_concurrency", self.DEFAULT_GENERATION_CONCURRENCY)
        try:
            return self._validate_generation_concurrency(value)
        except ValueError:
            return self.DEFAULT_GENERATION_CONCURRENCY

    def _public_model_config(self, kind: str) -> dict[str, Any]:
        """Return one model configuration without returning its API key."""

        configured = self._refresh_setting(kind)
        if kind == "video" and not configured:
            shared = self.settings.get("multimodal", {})
            legacy_video_model = shared.get("video_model") if isinstance(shared, dict) else None
            configured = dict(shared) if isinstance(shared, dict) else {}
            if legacy_video_model:
                configured["model"] = legacy_video_model
                configured["models"] = [legacy_video_model]
            else:
                configured.pop("model", None)
                configured.pop("models", None)
        if not isinstance(configured, dict):
            configured = {}
        has_configured_models = "models" in configured
        raw_models = configured.get("models")
        models = raw_models if isinstance(raw_models, list) else []
        normalized_models = list(dict.fromkeys(str(value).strip() for value in models if str(value).strip()))
        default_model = str(configured.get("model") or "").strip()
        if not normalized_models and not has_configured_models:
            normalized_models = list(self.MODEL_DEFAULTS.get(kind, []))
        if not default_model and normalized_models:
            default_model = normalized_models[0]
        api_key = str(configured.get("api_key") or os.getenv("OPENAI_API_KEY") or "")
        payload = {
            "kind": kind,
            "endpoint": str(configured.get("endpoint") or ""),
            "model": default_model,
            "models": normalized_models,
            "api_key_set": bool(api_key),
            "api_key_masked": self._mask_secret(api_key),
            "create_url": str(configured.get("create_url") or ""),
            "query_url": str(configured.get("query_url") or ""),
            "provider": str(configured.get("provider") or "ark"),
            "region": str(configured.get("region") or ""),
            "secret_id_masked": self._mask_secret(configured.get("secret_id")),
            "secret_key_set": bool(configured.get("secret_key")),
            "app_id": str(configured.get("app_id") or ""),
            "resource_id": str(configured.get("resource_id") or ""),
            "voice": str(configured.get("voice") or ""),
        }
        if kind in {"language", "multimodal", "video", "audio"}:
            payload["generation_concurrency"] = self._configured_generation_concurrency(configured)
        return payload

    @staticmethod
    def _mask_secret(value: Any) -> str:
        """Return a fixed-length mask suitable for the settings page."""

        secret = str(value or "")
        return "" if not secret else "*" * max(8, min(16, len(secret)))

    def get_model_configs(self) -> dict[str, dict[str, Any]]:
        """Return every model card's public configuration to the settings page."""

        return {kind: self._public_model_config(kind) for kind in self.MODEL_DEFAULTS}

    def save_model_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Probe and persist one provider card with its queue concurrency."""

        kind = str(config["kind"])
        previous = self._refresh_setting(kind)
        previous = previous if isinstance(previous, dict) else {}
        normalized = dict(config)
        previous_provider = str(previous.get("provider") or "ark").lower()
        provider = normalized_provider(normalized.get("provider") or previous_provider)
        provider_changed = provider != previous_provider
        for key in ("api_key", "secret_id", "secret_key", "region", "app_id", "resource_id", "voice", "endpoint", "create_url", "query_url"):
            if not normalized.get(key) and previous.get(key) and not provider_changed:
                normalized[key] = previous[key]
        for key, value in provider_defaults(kind, provider).items():
            if provider_changed or not normalized.get(key):
                normalized[key] = value
        normalized["provider"] = provider
        model = str(normalized.get("model") or normalized.get("video_model") or "").strip()
        raw_models = normalized.get("models") if isinstance(normalized.get("models"), list) else previous.get("models", [])
        models = list(dict.fromkeys(str(value).strip() for value in raw_models if str(value).strip()))
        if model and model not in models:
            models.insert(0, model)
        normalized["model"] = model or (models[0] if models else "")
        normalized["models"] = models
        normalized.pop("video_model", None)
        if kind in {"language", "multimodal", "video", "audio"}:
            value = normalized.get("generation_concurrency")
            normalized["generation_concurrency"] = self._validate_generation_concurrency(
                self._configured_generation_concurrency(previous) if value is None else value
            )
        self._probe_model_config(normalized)
        self.settings[kind] = normalized
        self.repository.set_setting(kind, normalized)
        if kind == "language" and isinstance(self.planner, ScriptPlanner):
            self.planner.configure(
                {"api_key": normalized.get("api_key"), "endpoint": normalized.get("endpoint"), "model": normalized.get("model"), "provider": normalized.get("provider")}
            )
        return {"status": "saved", "kind": kind, **self._public_model_config(kind)}
