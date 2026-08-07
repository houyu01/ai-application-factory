"""Regression coverage for the mandatory AI-generated image label."""

from src.application.task_service import TaskService
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.client.ark_client import ArkClient


def test_every_provider_image_prompt_includes_the_ai_generated_tag(tmp_path, monkeypatch):
    """Every generated image must carry the required upper-left provenance label."""

    service = TaskService(SQLiteRepository(tmp_path / "tag.db"), object())
    received_prompts: list[str] = []

    def generate_image(_self, prompt, **_kwargs):
        received_prompts.append(prompt)
        return {"url": "https://images.example/generated.png"}

    monkeypatch.setattr(ArkClient, "generate_image", generate_image)

    service._generate_provider_image(
        {"provider": "ark", "api_key": "test-key", "model": "doubao-seedream"},
        "生成一张电影感人物海报。",
    )

    assert len(received_prompts) == 1
    assert "左上角添加“AI生成”标签" in received_prompts[0]
    assert "圆角矩形" in received_prompts[0]
