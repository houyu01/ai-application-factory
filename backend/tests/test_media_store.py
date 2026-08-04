from pathlib import Path

import pytest

from src.application.task_service import TaskService
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.infrastructure.media_store import MediaStore, media_store


class FakeObjectClient:
    def __init__(self) -> None:
        self.calls: list[dict] = []

    def put_object(self, **kwargs) -> None:
        self.calls.append(kwargs)

    def delete_object(self, **kwargs) -> None:
        self.calls.append({"delete": True, **kwargs})


class FakeDownloadResponse:
    """Context-managed HTTP response returned by storage access probes."""

    def __init__(self, content: bytes) -> None:
        self.content = content

    def __enter__(self):
        return self

    def __exit__(self, *args) -> None:
        return None

    def read(self) -> bytes:
        return self.content


def test_local_media_store_returns_static_api_url(tmp_path: Path):
    store = MediaStore(tmp_path)

    url = store.save(b"video", ".mp4")

    assert url.startswith("/api/media/")
    media_id = url.rsplit("/", 1)[-1]
    assert store.path_for(media_id) == tmp_path / media_id
    assert (tmp_path / media_id).read_bytes() == b"video"


def test_local_media_requires_public_base_url_for_provider_references(tmp_path: Path):
    """A remote video provider cannot consume a laptop-only media route."""

    store = MediaStore(tmp_path)
    assert store.provider_reference_url("/api/media/character.png") is None

    store.configure({"provider": "local", "public_base_url": "https://studio.example.com"})
    assert store.provider_reference_url("/api/media/character.png") == (
        "https://studio.example.com/api/media/character.png"
    )
    assert store.provider_reference_url("https://cdn.example.com/character.png") == (
        "https://cdn.example.com/character.png"
    )


def test_local_media_accepts_public_cloud_request_origin(tmp_path: Path):
    """A cloud-hosted local store can expose media through the current API host."""

    store = MediaStore(tmp_path)
    assert store.provider_reference_url(
        "/api/media/character.png", "https://studio.example.com:8090"
    ) == "https://studio.example.com:8090/api/media/character.png"
    assert store.provider_reference_url(
        "/api/media/character.png", "http://8.8.8.8:8090"
    ) == "http://8.8.8.8:8090/api/media/character.png"


@pytest.mark.parametrize(
    "base_url",
    [
        "http://localhost:8090",
        "http://127.0.0.1:8090",
        "http://192.168.1.20:8090",
        "http://host.internal:8090",
    ],
)
def test_local_media_rejects_non_public_request_origins(
    tmp_path: Path, base_url: str
):
    """Loopback and private-network URLs are not reachable by a video provider."""

    store = MediaStore(tmp_path)
    assert store.provider_reference_url("/api/media/character.png", base_url) is None


def test_cos_media_store_uploads_through_s3_compatible_client(tmp_path: Path):
    client = FakeObjectClient()

    def factory(*args, **kwargs):
        assert args == ("s3",)
        assert kwargs["endpoint_url"] == "https://cos.ap-beijing.myqcloud.com"
        return client

    store = MediaStore(tmp_path, client_factory=factory)
    store.configure(
        {
            "provider": "cos",
            "endpoint": "https://cos.ap-beijing.myqcloud.com",
            "bucket": "demo-1250000000",
            "region": "ap-beijing",
            "secret_id": "secret-id",
            "secret_key": "secret-key",
            "public_base_url": "https://cdn.example.com/media",
        }
    )

    url = store.save(b"video", ".mp4")

    assert url.startswith("https://cdn.example.com/media/media/")
    assert len(client.calls) == 1
    assert client.calls[0]["Bucket"] == "demo-1250000000"
    assert client.calls[0]["Key"].startswith("media/")
    assert client.calls[0]["Body"] == b"video"
    assert client.calls[0]["ContentType"] == "video/mp4"
    safe_config = store.public_config()
    assert safe_config["secret_key_set"] is True
    assert "secret-key" not in str(safe_config)


def test_cos_normalizes_bucket_hostname_without_scheme(tmp_path: Path):
    """A COS console hostname is converted into a boto3 service endpoint."""

    client = FakeObjectClient()

    def factory(*args, **kwargs):
        assert args == ("s3",)
        assert kwargs["endpoint_url"] == "https://cos.ap-chengdu.myqcloud.com"
        return client

    store = MediaStore(tmp_path, client_factory=factory)
    store.configure(
        {
            "provider": "cos",
            "endpoint": "monkey-1256112104.cos.ap-chengdu.myqcloud.com",
            "bucket": "monkey-1256112104",
            "region": "ap-chengdu",
            "secret_id": "secret-id",
            "secret_key": "secret-key",
        }
    )

    url = store.save(b"image", ".png")

    assert store.public_config()["endpoint"] == (
        "https://cos.ap-chengdu.myqcloud.com"
    )
    assert url.startswith(
        "https://monkey-1256112104.cos.ap-chengdu.myqcloud.com/media/"
    )


def test_remote_storage_probe_uploads_downloads_and_cleans_up(
    tmp_path: Path, monkeypatch
):
    """Saving cloud settings verifies both write and public read access."""

    client = FakeObjectClient()
    store = MediaStore(tmp_path, client_factory=lambda *args, **kwargs: client)
    monkeypatch.setattr(
        "src.infrastructure.media_store.urlopen",
        lambda request, timeout: FakeDownloadResponse(
            b"ai-application-factory-storage-probe"
        ),
    )

    normalized = store.probe_config(
        {
            "provider": "cos",
            "endpoint": "cos.ap-chengdu.myqcloud.com",
            "bucket": "monkey-1256112104",
            "region": "ap-chengdu",
            "secret_id": "secret-id",
            "secret_key": "secret-key",
        }
    )

    assert normalized["endpoint"] == "https://cos.ap-chengdu.myqcloud.com"
    assert client.calls[0]["ContentType"] == "text/plain"
    assert client.calls[-1]["delete"] is True
    assert store.is_local is True


def test_remote_storage_probe_rejects_inaccessible_object(
    tmp_path: Path, monkeypatch
):
    """An uploaded object must be downloadable before settings are accepted."""

    client = FakeObjectClient()
    store = MediaStore(tmp_path, client_factory=lambda *args, **kwargs: client)

    def fail_download(request, timeout):
        raise RuntimeError("HTTP 403 Forbidden")

    monkeypatch.setattr("src.infrastructure.media_store.urlopen", fail_download)
    with pytest.raises(ValueError, match="嗅探访问失败.*403"):
        store.probe_config(
            {
                "provider": "oss",
                "endpoint": "oss-cn-hangzhou.aliyuncs.com",
                "bucket": "drama-assets",
                "region": "cn-hangzhou",
                "secret_id": "access-key-id",
                "secret_key": "access-key-secret",
            }
        )

    assert client.calls[-1]["delete"] is True
    assert store.is_local is True


def test_cos_media_store_deletes_only_objects_under_configured_prefix(tmp_path: Path):
    client = FakeObjectClient()
    store = MediaStore(tmp_path, client_factory=lambda *args, **kwargs: client)
    store.configure(
        {
            "provider": "cos",
            "endpoint": "https://cos.ap-beijing.myqcloud.com",
            "bucket": "demo-1250000000",
            "region": "ap-beijing",
            "secret_id": "secret-id",
            "secret_key": "secret-key",
            "public_base_url": "https://cdn.example.com/media",
        }
    )

    assert store.delete_url("https://cdn.example.com/media/media/video.mp4") is True
    assert store.delete_url("https://other.example.com/media/video.mp4") is False
    assert client.calls[-1] == {
        "delete": True,
        "Bucket": "demo-1250000000",
        "Key": "media/video.mp4",
    }


def test_oss_media_store_uses_s3_virtual_hosted_upload_and_delete(tmp_path: Path):
    """Alibaba OSS uses its S3-compatible API with virtual-hosted URLs."""

    client = FakeObjectClient()

    def factory(*args, **kwargs):
        assert args == ("s3",)
        assert kwargs["endpoint_url"] == "https://oss-cn-hangzhou.aliyuncs.com"
        assert kwargs["region_name"] == "cn-hangzhou"
        assert kwargs["config"].s3 == {"addressing_style": "virtual"}
        return client

    store = MediaStore(tmp_path, client_factory=factory)
    store.configure(
        {
            "provider": "oss",
            "endpoint": "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket": "drama-assets",
            "region": "cn-hangzhou",
            "secret_id": "access-key-id",
            "secret_key": "access-key-secret",
        }
    )

    url = store.save(b"image", ".png")

    assert url.startswith(
        "https://drama-assets.oss-cn-hangzhou.aliyuncs.com/media/"
    )
    assert client.calls[0]["Bucket"] == "drama-assets"
    assert client.calls[0]["ContentType"] == "image/png"
    assert store.delete_url(url) is True
    assert client.calls[-1] == {
        "delete": True,
        "Bucket": "drama-assets",
        "Key": url.split(".com/", 1)[1],
    }


def test_storage_config_is_persisted_without_exposing_secret_key(
    tmp_path: Path, monkeypatch
):
    repository = SQLiteRepository(tmp_path / "settings.db")
    service = TaskService(repository, planner=object())
    monkeypatch.setattr(media_store, "probe_config", lambda config: config)

    saved = service.save_storage_config(
        {
            "provider": "tos",
            "endpoint": "https://tos-cn-beijing.volces.com",
            "bucket": "demo",
            "region": "cn-beijing",
            "secret_id": "access-id",
            "secret_key": "access-secret",
        }
    )
    restarted = TaskService(repository, planner=object())

    assert saved["provider"] == "tos"
    assert saved["secret_key_set"] is True
    assert restarted.get_storage_config()["provider"] == "tos"
    assert "access-secret" not in str(restarted.get_storage_config())


def test_storage_probe_failure_does_not_replace_persisted_config(
    tmp_path: Path, monkeypatch
):
    """A failed cloud probe leaves the previous storage settings untouched."""

    repository = SQLiteRepository(tmp_path / "failed-storage-probe.db")
    service = TaskService(repository, planner=object())

    def fail_probe(config):
        raise ValueError("COS 媒体存储嗅探访问失败：HTTP 403 Forbidden")

    monkeypatch.setattr(media_store, "probe_config", fail_probe)
    with pytest.raises(ValueError, match="403 Forbidden"):
        service.save_storage_config(
            {
                "provider": "cos",
                "endpoint": "cos.ap-chengdu.myqcloud.com",
                "bucket": "monkey-1256112104",
                "region": "ap-chengdu",
                "secret_id": "secret-id",
                "secret_key": "secret-key",
            }
        )

    assert repository.get_setting("storage") is None
    assert service.get_storage_config()["provider"] == "local"


def test_provider_result_reloads_storage_config_for_long_running_worker(
    tmp_path: Path, monkeypatch
):
    """A worker reloads storage settings saved by another API process."""

    repository = SQLiteRepository(tmp_path / "worker-storage.db")
    service = TaskService(repository, planner=object())
    config = {
        "provider": "cos",
        "endpoint": "monkey-1256112104.cos.ap-chengdu.myqcloud.com",
        "bucket": "monkey-1256112104",
        "secret_id": "secret-id",
        "secret_key": "secret-key",
    }
    repository.set_setting("storage", config)
    configured: list[dict] = []
    monkeypatch.setattr(media_store, "configure", configured.append)
    monkeypatch.setattr(media_store, "save", lambda *args, **kwargs: "https://media.example/image.png")

    result = service._persist_provider_result(
        {"content": b"image"}, ".png", "图片模型"
    )

    assert configured == [config]
    assert result == "https://media.example/image.png"
