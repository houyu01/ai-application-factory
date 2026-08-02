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
