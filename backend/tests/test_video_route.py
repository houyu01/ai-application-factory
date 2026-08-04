"""Regression coverage for the short-drama video task API route."""

from fastapi.testclient import TestClient
from starlette.requests import Request

from src.api import router
from src.main import app


def public_request() -> Request:
    """Build the public request shape used by a cloud-hosted API."""

    return Request(
        {
            "type": "http",
            "method": "POST",
            "scheme": "https",
            "server": ("studio.example.com", 443),
            "path": "/",
            "root_path": "",
            "query_string": b"",
            "headers": [(b"host", b"studio.example.com")],
        }
    )


def test_generate_shot_video_enqueues_a_durable_task(monkeypatch) -> None:
    """The editor's Generate Video button must receive a durable task, not a 404."""

    expected = {
        "id": "video-task-1",
        "type": "shot_video",
        "status": "生成中",
        "project_id": "project-1",
        "resource_id": "shot-1",
        "created_at": "2026-08-04T00:00:00Z",
        "progress": 0,
        "stage": "",
    }
    monkeypatch.setattr(
        router.task_service,
        "enqueue",
        lambda kind, project_id, shot_id, public_media_base_url=None: {
            **expected,
            "type": kind,
            "project_id": project_id,
            "resource_id": shot_id,
        },
    )

    assert router.generate_shot_video(
        "project-1", "shot-1", public_request()
    ) == expected


def test_generate_shot_video_is_exposed_over_http(monkeypatch) -> None:
    """The browser request must resolve to 202 instead of a missing-route 404."""

    monkeypatch.setattr(
        router.task_service,
        "enqueue",
        lambda kind, project_id, shot_id, public_media_base_url=None: {
            "id": "video-task-2",
            "type": kind,
            "status": "生成中",
            "project_id": project_id,
            "resource_id": shot_id,
            "created_at": "2026-08-04T00:00:00Z",
            "progress": 0,
            "stage": "",
        },
    )

    response = TestClient(app).post("/api/projects/project-2/shots/shot-2/video")

    assert response.status_code == 202
    assert response.json()["id"] == "video-task-2"


def test_cloud_request_origin_is_forwarded_to_durable_video_task(monkeypatch) -> None:
    """Local media remains usable when the API itself has a public cloud origin."""

    captured: dict[str, str | None] = {}

    def enqueue(kind, project_id, shot_id, public_media_base_url=None):
        captured["base_url"] = public_media_base_url
        return {
            "id": "video-task-3", "type": kind, "status": "生成中",
            "project_id": project_id, "resource_id": shot_id,
            "created_at": "2026-08-04T00:00:00Z", "progress": 0, "stage": "",
        }

    monkeypatch.setattr(router.task_service, "enqueue", enqueue)
    client = TestClient(app, base_url="https://studio.example.com")
    response = client.post("/api/projects/project-3/shots/shot-3/video")

    assert response.status_code == 202
    assert captured["base_url"] == "https://studio.example.com"
