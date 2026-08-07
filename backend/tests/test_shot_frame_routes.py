"""Regression coverage for saving a shot's first and last frame references."""

from fastapi.testclient import TestClient

from src.api import router
from src.main import app


def test_update_shot_accepts_and_forwards_both_boundary_frames(monkeypatch) -> None:
    """The frame editor's completion action must persist both selected image references."""

    captured: dict[str, object] = {}

    def update_shot(project_id: str, shot_id: str, **fields: object) -> dict[str, object]:
        captured.update(project_id=project_id, shot_id=shot_id, **fields)
        return {"id": shot_id, **fields}

    monkeypatch.setattr(router.task_service.repository, "update_shot", update_shot)
    frames = {
        "first": {"url": "data:image/jpeg;base64,first", "source": "frame"},
        "last": {"url": "data:image/jpeg;base64,last", "source": "frame"},
    }

    response = TestClient(app).put(
        "/api/projects/project-frames/shots/shot-frames",
        json={"first_last_frames": frames},
    )

    assert response.status_code == 200
    assert captured == {
        "project_id": "project-frames",
        "shot_id": "shot-frames",
        "first_last_frames": frames,
    }
