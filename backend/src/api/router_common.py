"""Shared API router context for drama and interactive-game route modules."""

from fastapi import APIRouter, Request

from ..application.drama_gateway import drama_gateway
from ..application.game_gateway import game_gateway
from ..application.game_service import game_service
from ..application.task_service import task_service
from ..infrastructure.media_store import media_store

api_router = APIRouter()


def request_public_media_base_url(request: Request) -> str | None:
    """Resolve the public API origin used by remote media-generation providers."""

    forwarded_host = request.headers.get("x-forwarded-host", "").split(",", 1)[0].strip()
    forwarded_proto = request.headers.get("x-forwarded-proto", "").split(",", 1)[0].strip()
    if forwarded_host:
        candidate = f"{forwarded_proto or request.url.scheme}://{forwarded_host}"
    else:
        candidate = str(request.base_url)
    return media_store.public_request_base_url(candidate)

__all__ = [
    "api_router",
    "drama_gateway",
    "game_gateway",
    "game_service",
    "media_store",
    "request_public_media_base_url",
    "task_service",
]
