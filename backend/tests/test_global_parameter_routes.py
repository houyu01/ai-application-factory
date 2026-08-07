"""Regression coverage for saving short-drama global parameters."""

from src.api import router
from src.domain.models import ProjectParametersUpdate


def test_global_parameter_save_does_not_enqueue_generation_tasks(monkeypatch) -> None:
    """The Global Parameters save action must persist values without starting work."""

    saved: dict[str, object] = {}

    def update(project_id: str, values: dict[str, object]) -> dict[str, object]:
        saved.update({"project_id": project_id, **values})
        return saved

    monkeypatch.setattr(router.task_service.repository, "update_project_parameters", update)
    monkeypatch.setattr(
        router.task_service,
        "enqueue",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("must not enqueue")),
    )

    result = router.update_project_parameters(
        "project-1", ProjectParametersUpdate(ratio="16:9", theme="悬疑")
    )

    assert result == {"project_id": "project-1", "ratio": "16:9", "theme": "悬疑"}
