"""ORM persistence for reusable short-drama assets and their variants."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from sqlalchemy import select

from ..domain.models import GenerationStatus
from .orm_models import DramaAsset
from .repository_common import _json_dump, _json_load, utc_now


class DramaRepositoryAssetMixin:
    """Manage reusable visual assets, uploaded references, and cover outputs.

    Asset drawers, shot reference pickers, and the cover dialog call this mixin
    when users add, edit, upload, generate, or select visual assets. ORM
    transactions keep image history, variants, voice settings, cover metadata,
    and task-visible status together.
    """

    def _asset_model(self, session: Any, drama_id: str, asset_id: str) -> DramaAsset | None:
        asset = session.get(DramaAsset, asset_id)
        if asset is None or asset.drama_id != drama_id:
            return None
        return asset

    def get_asset(self, drama_id: str, asset_id: str) -> dict[str, Any] | None:
        with self.database.session() as session:
            asset = self._asset_model(session, drama_id, asset_id)
            return self._asset_from_row(asset) if asset else None

    def list_assets(self, drama_id: str) -> list[dict[str, Any]]:
        """Return only this drama's assets for a partial task-completion refresh."""
        with self.database.session() as session:
            assets = session.scalars(
                select(DramaAsset)
                .where(DramaAsset.drama_id == drama_id)
                .order_by(DramaAsset.created_at, DramaAsset.id)
            ).all()
            return [self._asset_from_row(asset) for asset in assets]

    def create_asset(
        self,
        drama_id: str,
        asset_type: str,
        name: str,
        prompt: str = "",
        metadata: dict[str, Any] | None = None,
        voice_id: str | None = None,
        content_hash: str | None = None,
        source_type: str = "generated",
    ) -> dict[str, Any]:
        normalized_voice_id = str(voice_id or "").strip() or None
        if normalized_voice_id and normalized_voice_id != "none":
            if self.get_voice_preset(normalized_voice_id) is None:
                raise ValueError(f"Voice preset not found: {normalized_voice_id}")
        else:
            normalized_voice_id = None
        timestamp = utc_now()
        asset = DramaAsset(
            id=str(uuid4()), drama_id=drama_id, type=asset_type,
            name=name.strip(), prompt=prompt.strip(), voice_id=normalized_voice_id,
            image_url=None, content_hash=content_hash, source_type=source_type,
            image_history_json="[]", variants_json="[]",
            metadata_json=_json_dump(metadata or {}),
            status=GenerationStatus.NOT_GENERATED.value,
            created_at=timestamp, updated_at=timestamp,
        )
        with self.database.session() as session:
            session.add(asset)
            session.flush()
            return self._asset_from_row(asset)

    def delete_asset(self, drama_id: str, asset_id: str) -> None:
        with self.database.session() as session:
            asset = self._asset_model(session, drama_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            session.delete(asset)

    @staticmethod
    def _asset_variant(asset: dict[str, Any], variant_id: str) -> dict[str, Any] | None:
        return next(
            (variant for variant in asset.get("variants", []) if str(variant.get("id")) == variant_id),
            None,
        )

    def _update_variants(self, drama_id: str, asset_id: str, variants: list[dict[str, Any]]) -> dict[str, Any]:
        with self.database.session() as session:
            asset = self._asset_model(session, drama_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            asset.variants_json = _json_dump(variants)
            asset.updated_at = utc_now()
            session.flush()
            return self._asset_from_row(asset)

    def create_asset_variant(self, drama_id: str, asset_id: str, name: str, prompt: str = "") -> dict[str, Any]:
        asset = self.get_asset(drama_id, asset_id)
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        timestamp = utc_now()
        variant = {
            "id": str(uuid4()), "name": name.strip(), "prompt": prompt.strip(),
            "image_url": None, "image_history": [],
            "status": GenerationStatus.NOT_GENERATED.value,
            "created_at": timestamp, "updated_at": timestamp,
        }
        return self._update_variants(drama_id, asset_id, [*asset.get("variants", []), variant])

    def update_asset_variant(
        self, drama_id: str, asset_id: str, variant_id: str, *,
        name: str | None = None, prompt: str | None = None,
    ) -> dict[str, Any]:
        asset = self.get_asset(drama_id, asset_id)
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        variants = list(asset.get("variants", []))
        if self._asset_variant(asset, variant_id) is None:
            raise KeyError(f"Asset variant not found: {variant_id}")
        for variant in variants:
            if str(variant.get("id")) == variant_id:
                if name is not None:
                    variant["name"] = name.strip()
                if prompt is not None:
                    variant["prompt"] = prompt.strip()
                variant["updated_at"] = utc_now()
        return self._update_variants(drama_id, asset_id, variants)

    def delete_asset_variant(self, drama_id: str, asset_id: str, variant_id: str) -> dict[str, Any]:
        asset = self.get_asset(drama_id, asset_id)
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        variants = [v for v in asset.get("variants", []) if str(v.get("id")) != variant_id]
        if len(variants) == len(asset.get("variants", [])):
            raise KeyError(f"Asset variant not found: {variant_id}")
        return self._update_variants(drama_id, asset_id, variants)

    def update_asset_variant_status(
        self, drama_id: str, asset_id: str, variant_id: str,
        status: GenerationStatus, image_url: str | None = None,
    ) -> dict[str, Any]:
        asset = self.get_asset(drama_id, asset_id)
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        variants = list(asset.get("variants", []))
        if self._asset_variant(asset, variant_id) is None:
            raise KeyError(f"Asset variant not found: {variant_id}")
        for variant in variants:
            if str(variant.get("id")) != variant_id:
                continue
            variant["status"] = status.value
            variant["updated_at"] = utc_now()
            if image_url:
                variant["image_url"] = image_url
                history = list(variant.get("image_history", []))
                history.append({"id": str(uuid4()), "url": image_url, "generated_at": utc_now()})
                variant["image_history"] = history
        return self._update_variants(drama_id, asset_id, variants)

    def update_asset_status(self, asset_id: str, status: GenerationStatus, image_url: str | None = None) -> None:
        with self.database.session() as session:
            asset = session.get(DramaAsset, asset_id)
            if asset is None:
                return
            history = _json_load(asset.image_history_json, [])
            if not isinstance(history, list):
                history = []
            if image_url:
                history.append({"id": str(uuid4()), "url": image_url, "generated_at": utc_now()})
            asset.status = status.value
            asset.image_url = image_url or asset.image_url
            asset.image_history_json = _json_dump(history)
            asset.updated_at = utc_now()

    def find_asset_by_content_hash(self, drama_id: str, content_hash: str) -> dict[str, Any] | None:
        with self.database.session() as session:
            asset = session.scalars(
                select(DramaAsset).where(
                    DramaAsset.drama_id == drama_id,
                    DramaAsset.content_hash == content_hash,
                ).order_by(DramaAsset.created_at, DramaAsset.id).limit(1)
            ).first()
            return self._asset_from_row(asset) if asset else None

    def update_asset(
        self, drama_id: str, asset_id: str, *, name: str | None = None,
        prompt: str | None = None, image_url: str | None = None,
        voice_id: str | None = None,
    ) -> dict[str, Any]:
        with self.database.session() as session:
            asset = self._asset_model(session, drama_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            if name is not None:
                asset.name = name.strip()
            if prompt is not None:
                asset.prompt = prompt.strip()
            if image_url is not None:
                asset.image_url = image_url
            if voice_id is not None:
                normalized = str(voice_id).strip()
                if normalized and normalized != "none" and self.get_voice_preset(normalized) is None:
                    raise ValueError(f"Voice preset not found: {normalized}")
                asset.voice_id = normalized or None
            asset.updated_at = utc_now()
            session.flush()
            return self._asset_from_row(asset)

    def set_asset_image(
        self, drama_id: str, asset_id: str, image_url: str, *,
        content_hash: str | None = None, source_type: str = "uploaded",
    ) -> dict[str, Any]:
        with self.database.session() as session:
            asset = self._asset_model(session, drama_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            history = _json_load(asset.image_history_json, [])
            if not isinstance(history, list):
                history = []
            history.append({"id": str(uuid4()), "url": image_url, "generated_at": utc_now()})
            asset.image_url = image_url
            asset.content_hash = content_hash or asset.content_hash
            asset.source_type = source_type
            asset.status = GenerationStatus.SUCCEEDED.value
            asset.image_history_json = _json_dump(history)
            asset.updated_at = utc_now()
            session.flush()
            return self._asset_from_row(asset)
