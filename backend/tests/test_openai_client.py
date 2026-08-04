from __future__ import annotations

import asyncio
from types import SimpleNamespace

from src.llm_service.client.openai_client import OpenAICLient


class FakeItem:
    def __init__(self, **payload):
        self.__dict__.update(payload)
        self.payload = payload

    def model_dump(self, exclude_none: bool = False):
        return dict(self.payload)


class FakeResponse:
    def __init__(self, output, output_text: str = ""):
        self.output = output
        self.output_text = output_text


class FakeSyncResponses:
    def __init__(self, responses):
        self.responses = iter(responses)
        self.requests = []

    def create(self, **request):
        self.requests.append(request)
        return next(self.responses)


class FakeSyncClient:
    def __init__(self, responses):
        self.responses = FakeSyncResponses(responses)


class FakeImageResponse:
    def __init__(self, data):
        self.data = data


class FakeImages:
    def __init__(self, response):
        self.response = response
        self.requests = []

    def generate(self, **request):
        self.requests.append(request)
        return self.response


class FakeVideoDownload:
    def read(self):
        return b"video-bytes"


class FakeVideos:
    def __init__(self):
        self.create_requests = []
        self.download_requests = []

    def create_and_poll(self, **request):
        self.create_requests.append(request)
        return FakeItem(status="completed", id="video_1")

    def download_content(self, video_id, variant):
        self.download_requests.append((video_id, variant))
        return FakeVideoDownload()


class FakeAsyncStream:
    def __init__(self, events):
        self.events = events

    def __aiter__(self):
        return self._iterate()

    async def _iterate(self):
        for event in self.events:
            yield event


class FakeAsyncResponses:
    def __init__(self, streams):
        self.streams = iter(streams)
        self.requests = []

    async def create(self, **request):
        self.requests.append(request)
        return next(self.streams)


class FakeAsyncClient:
    def __init__(self, streams):
        self.responses = FakeAsyncResponses(streams)


def make_client(sync_client=None, async_client=None):
    return OpenAICLient(
        {"api_key": "test-key", "model": "test-model"},
        sync_client=sync_client,
        client=async_client,
    )


def test_completion_uses_sync_client_and_returns_text_after_tool_round():
    tool_call = FakeItem(
        id="fc_1",
        type="function_call",
        call_id="call_1",
        name="lookup",
        arguments='{"value": 2}',
    )
    first = FakeResponse([tool_call])
    second = FakeResponse([], output_text="完成")
    sync_client = FakeSyncClient([first, second])
    client = make_client(sync_client=sync_client)

    result = client.completion(
        [{"role": "user", "content": "开始"}],
        tools=[
            {
                "type": "function",
                "function": {
                    "name": "lookup",
                    "parameters": {"type": "object"},
                },
            }
        ],
        tool_executor=lambda name, args: {"name": name, "value": args["value"]},
    )

    assert result == "完成"
    assert len(sync_client.responses.requests) == 2
    assert client.history[-1]["type"] == "function_call_output"


def test_completion_stream_yields_text_and_executes_tool():
    asyncio.run(_test_completion_stream_yields_text_and_executes_tool())


def test_generate_image_returns_provider_url():
    sync_client = FakeSyncClient([])
    sync_client.images = FakeImages(
        FakeImageResponse([FakeItem(url="https://cdn.example/image.png")])
    )
    client = make_client(sync_client=sync_client)

    result = client.generate_image("一只鸟", size="1024x1024")

    assert result["url"] == "https://cdn.example/image.png"
    assert sync_client.images.requests[0]["model"] == "test-model"


def test_generate_video_polls_and_downloads_provider_content():
    sync_client = FakeSyncClient([])
    sync_client.videos = FakeVideos()
    client = make_client(sync_client=sync_client)

    result = client.generate_video("一只鸟飞过森林", ratio="9:16", seconds=10)

    assert result["content"] == b"video-bytes"
    assert result["seconds"] == 8
    assert sync_client.videos.create_requests[0]["size"] == "720x1280"
    assert sync_client.videos.download_requests == [("video_1", "video")]


def test_generate_video_passes_first_rich_prompt_reference_to_openai():
    sync_client = FakeSyncClient([])
    sync_client.videos = FakeVideos()
    client = make_client(sync_client=sync_client)

    client.generate_video(
        "连续长镜头",
        ratio="9:16",
        reference_images=[
            "https://cdn.example/character.png",
            "https://cdn.example/scene.png",
        ],
    )

    assert sync_client.videos.create_requests[0]["input_reference"] == {
        "image_url": "https://cdn.example/character.png"
    }


async def _test_completion_stream_yields_text_and_executes_tool():
    tool_call = FakeItem(
        id="fc_1",
        type="function_call",
        call_id="call_1",
        name="lookup",
        arguments='{"value": 2}',
    )
    first_stream = FakeAsyncStream(
        [
            SimpleNamespace(
                type="response.output_item.added",
                item=FakeItem(
                    id="fc_1",
                    type="function_call",
                    call_id="call_1",
                    name="lookup",
                    arguments="",
                ),
            ),
            SimpleNamespace(
                type="response.function_call_arguments.done",
                item_id="fc_1",
                arguments='{"value": 2}',
            ),
            SimpleNamespace(type="response.output_item.done", item=tool_call),
        ]
    )
    second_stream = FakeAsyncStream(
        [
            SimpleNamespace(type="response.output_text.delta", delta="流式"),
            SimpleNamespace(type="response.output_text.delta", delta="完成"),
        ]
    )
    async_client = FakeAsyncClient([first_stream, second_stream])
    client = make_client(async_client=async_client)

    called = []

    async def execute(name, args):
        called.append((name, args))
        return {"ok": True}

    chunks = [
        chunk
        async for chunk in client.completion_stream(
            [{"role": "user", "content": "开始"}],
            tool_executor=execute,
        )
    ]

    assert chunks == ["流式", "完成"]
    assert called == [("lookup", {"value": 2})]
    assert len(async_client.responses.requests) == 2
