"""Small persistence helpers shared by repository modules."""

import json
from datetime import datetime, timezone
from typing import Any

from sqlalchemy import inspect

JSON_FIELDS = {
    "shots_json": "shots",
    "assets_json": "assets",
    "historical_videos_json": "historical_videos",
    "asset_public_prompts_json": "asset_public_prompts",
    "shot_constraints_json": "shot_constraints",
    "result_json": "result",
}


def model_to_row(model: Any) -> dict[str, Any]:
    """Return a SQLAlchemy mapped instance as the row-shaped mapping used by adapters."""

    if isinstance(model, dict):
        return model
    inspected = inspect(model, raiseerr=False)
    if inspected is None:
        return dict(model)
    return {
        attribute.key: getattr(model, attribute.key)
        for attribute in inspected.mapper.column_attrs
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
