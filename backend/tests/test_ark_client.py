from __future__ import annotations

import json

from src.llm_service.client.ark_client import ArkClient


class FakeResponse:
    def __init__(self, payload):
        self.payload = payload

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self):
        return json.dumps(self.payload).encode()


def test_ark_image_generation_returns_url_and_protocol_payload():
    requests = []

    def opener(request, timeout):
        requests.append((request, timeout))
        return FakeResponse({"data": [{"url": "https://cdn.example/image.png"}]})

    result = ArkClient(
        api_key="ark-test-key",
        base_url="https://ark.cn-beijing.volces.com/api/v3/",
        model="doubao-seedream-test",
        opener=opener,
    ).generate_image("公共风格\n\n年轻主角")

    assert result["url"] == "https://cdn.example/image.png"
    assert requests[0][0].full_url.endswith("/images/generations")
    assert json.loads(requests[0][0].data) == {
        "model": "doubao-seedream-test",
        "prompt": "公共风格\n\n年轻主角",
        "size": "2K",
        "sequential_image_generation": "disabled",
        "response_format": "url",
        "watermark": False,
    }


def test_ark_video_generation_creates_and_polls_until_succeeded():
    responses = iter(
        [
            {"id": "task-1"},
            {"id": "task-1", "status": "running"},
            {
                "id": "task-1",
                "status": "succeeded",
                "content": {"video_url": "https://cdn.example/video.mp4"},
            },
        ]
    )
    paths = []

    def opener(request, timeout):
        paths.append(request.full_url)
        return FakeResponse(next(responses))

    result = ArkClient(
        api_key="ark-test-key",
        base_url="https://ark.cn-beijing.volces.com/api/v3",
        model="doubao-seedance-test",
        opener=opener,
    ).generate_video("分镜提示词", poll_interval=0, timeout=1)

    assert result["url"] == "https://cdn.example/video.mp4"
    assert paths == [
        "https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks",
        "https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks/task-1",
        "https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks/task-1",
    ]


def test_ark_video_generation_uses_configured_create_and_query_urls():
    responses = iter(
        [
            {"id": "remote-task/1"},
            {"id": "remote-task/1", "status": "succeeded", "video_url": "https://cdn.example/video.mp4"},
        ]
    )
    requests = []

    def opener(request, timeout):
        requests.append((request.full_url, request.method))
        return FakeResponse(next(responses))

    result = ArkClient(
        api_key="ark-test-key",
        model="custom-video-model",
        create_url="https://provider.example/create-task",
        query_url="https://provider.example/query/{id}",
        opener=opener,
    ).generate_video("分镜提示词", poll_interval=0, timeout=1)

    assert result["url"] == "https://cdn.example/video.mp4"
    assert requests == [
        ("https://provider.example/create-task", "POST"),
        ("https://provider.example/query/remote-task%2F1", "GET"),
    ]


def test_ark_video_cancel_uses_the_provider_delete_task_endpoint():
    """Cancelling a shot must call Ark's DeleteContentsGenerationsTasks API."""

    requests = []

    def opener(request, timeout):
        requests.append((request.full_url, request.method, request.data))
        return FakeResponse({"id": "remote-task/1", "status": "cancelled"})

    result = ArkClient(
        api_key="ark-test-key",
        model="custom-video-model",
        create_url="https://provider.example/create-task",
        query_url="https://provider.example/query/{id}",
        opener=opener,
    ).cancel_video_task("remote-task/1")

    assert result["status"] == "cancelled"
    assert requests == [
        ("https://provider.example/query/remote-task%2F1", "DELETE", None)
    ]


def test_ark_video_create_task_sends_prompt_and_reference_images():
    """Video generation submits rich prompt text together with selected asset images."""

    requests = []

    def opener(request, timeout):
        requests.append(request)
        return FakeResponse({"id": "task-with-references"})

    client = ArkClient(
        api_key="ark-test-key",
        model="custom-video-model",
        create_url="https://provider.example/create-task",
        query_url="https://provider.example/query/{id}",
        opener=opener,
    )
    result = client.create_video_task(
        "分镜提示词",
        ratio="9:16",
        resolution="720p",
        seconds=8,
        reference_images=["https://cdn.example/character.png", "https://cdn.example/scene.png"],
    )

    assert result["provider_task_id"] == "task-with-references"
    assert json.loads(requests[0].data) == {
        "model": "custom-video-model",
        "content": [
            {"type": "text", "text": "分镜提示词"},
            {
                "type": "image_url",
                "image_url": {"url": "https://cdn.example/character.png"},
                "role": "reference_image",
            },
            {
                "type": "image_url",
                "image_url": {"url": "https://cdn.example/scene.png"},
                "role": "reference_image",
            },
        ],
        "generate_audio": True,
        "ratio": "9:16",
        "duration": 8,
        "watermark": False,
        "resolution": "720p",
    }


def test_ark_video_create_task_sends_boundary_images_as_references():
    """Boundary frames stay in the one reference-image input mode Ark accepts."""

    requests = []

    def opener(request, timeout):
        requests.append(request)
        return FakeResponse({"id": "task-with-boundary-frames"})

    client = ArkClient(
        api_key="ark-test-key",
        base_url="https://ark.cn-beijing.volces.com/api/v3",
        model="doubao-seedance-test",
        opener=opener,
    )
    client.create_video_task(
        "@图2 是视频首帧，@图3 是视频尾帧。",
        reference_images=[
            "https://cdn.example/character.jpg",
            "https://cdn.example/first.jpg",
            "https://cdn.example/last.jpg",
        ],
    )

    content = json.loads(requests[0].data)["content"]
    assert content == [
        {
            "type": "text",
            "text": "@图2 是视频首帧，@图3 是视频尾帧。",
        },
        {
            "type": "image_url",
            "image_url": {"url": "https://cdn.example/character.jpg"},
            "role": "reference_image",
        },
        {
            "type": "image_url",
            "image_url": {"url": "https://cdn.example/first.jpg"},
            "role": "reference_image",
        },
        {
            "type": "image_url",
            "image_url": {"url": "https://cdn.example/last.jpg"},
            "role": "reference_image",
        },
    ]


def test_ark_video_poll_reads_the_plan_api_response_shape():
    """The durable worker can consume Ark's top-level status and content URL."""

    payload = {
        "id": "cgt-20260805135906-vkqzn",
        "model": "doubao-seedance-2.0",
        "status": "succeeded",
        "content": {"video_url": "https://cdn.example/generated.mp4"},
        "resolution": "720p",
        "ratio": "1:1",
        "duration": 5,
    }

    assert ArkClient._read_status(payload) == "succeeded"
    assert ArkClient._read_video_url(payload) == "https://cdn.example/generated.mp4"
