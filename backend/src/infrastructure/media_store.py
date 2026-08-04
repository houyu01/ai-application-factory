"""Media persistence for generated images and videos.

The application keeps one storage abstraction for all generated media.  Local
storage is the default so the project works without any cloud credentials.
Volcengine TOS, Tencent COS, and Alibaba Cloud OSS expose the S3-compatible
object operations used here, so one lazy boto3 client serves all providers.
"""

from __future__ import annotations

import mimetypes
import os
import ipaddress
import logging
from pathlib import Path
from typing import Any, Callable
from urllib.parse import quote, unquote, urlparse
from urllib.request import Request, urlopen
from uuid import uuid4

import boto3
from botocore.config import Config as BotoConfig


StorageConfig = dict[str, Any]
ClientFactory = Callable[..., Any]
logger = logging.getLogger(__name__)


class MediaStore:
    """Save generated media locally or to a configured object store.

    ``public_base_url`` is optional.  When it is provided, it is used to build
    the URL returned to the browser (useful for a CDN or a public bucket).  If
    it is empty, a virtual-hosted object URL is derived from the configured
    provider endpoint.
    """

    VALID_PROVIDERS = {"local", "tos", "cos", "oss"}

    def __init__(
        self,
        root: str | Path | None = None,
        client_factory: ClientFactory | None = None,
    ) -> None:
        default_root = Path(__file__).resolve().parents[2] / "data" / "media"
        self.root = Path(root or os.getenv("MEDIA_ROOT") or default_root)
        self.root.mkdir(parents=True, exist_ok=True)
        self._client_factory = client_factory or boto3.client
        self._config: StorageConfig = {"provider": "local", "prefix": "media"}
        self._client: Any | None = None
        self.configure({"provider": "local"})

    @property
    def provider(self) -> str:
        return str(self._config.get("provider") or "local")

    @property
    def is_local(self) -> bool:
        return self.provider == "local"

    def configure(self, config: StorageConfig | None) -> None:
        """Apply a validated storage configuration without making a network call."""

        values = dict(config or {})
        provider = str(values.get("provider") or "local").strip().lower()
        if provider not in self.VALID_PROVIDERS:
            raise ValueError("storage provider must be one of: local, tos, cos, oss")

        bucket = str(values.get("bucket") or "").strip()
        normalized: StorageConfig = {
            "provider": provider,
            "endpoint": self._normalize_endpoint(values.get("endpoint"), bucket),
            "bucket": bucket,
            "region": str(values.get("region") or "").strip(),
            "secret_id": str(values.get("secret_id") or "").strip(),
            "secret_key": str(values.get("secret_key") or "").strip(),
            "prefix": self._normalize_prefix(values.get("prefix") or "media"),
            "public_base_url": str(values.get("public_base_url") or "")
            .strip()
            .rstrip("/"),
        }
        if provider != "local":
            missing = [
                name
                for name in ("endpoint", "bucket", "secret_id", "secret_key")
                if not normalized[name]
            ]
            if missing:
                raise ValueError(
                    f"{provider} storage requires: {', '.join(missing)}"
                )
        if normalized == self._config:
            return
        self._config = normalized
        self._client = None

    @staticmethod
    def _normalize_endpoint(value: Any, bucket: str) -> str:
        """Normalize console-style object-store hosts for the S3 client.

        Configuration forms commonly receive a host without a URL scheme, or
        a provider object URL whose hostname already starts with the bucket.
        boto3 requires an absolute service endpoint and adds the bucket itself
        when virtual-hosted addressing is enabled.
        """

        endpoint = str(value or "").strip().rstrip("/")
        if not endpoint:
            return ""
        if "://" not in endpoint:
            endpoint = f"https://{endpoint}"
        parsed = urlparse(endpoint)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname:
            raise ValueError("storage endpoint must be a valid HTTP(S) URL or hostname")

        hostname = parsed.hostname
        bucket_prefix = f"{bucket}.".lower()
        if bucket and hostname.lower().startswith(bucket_prefix):
            service_hostname = hostname[len(bucket) + 1 :]
            port = f":{parsed.port}" if parsed.port else ""
            path = parsed.path.rstrip("/")
            endpoint = f"{parsed.scheme}://{service_hostname}{port}{path}"
        return endpoint.rstrip("/")

    @staticmethod
    def _normalize_prefix(value: Any) -> str:
        prefix = str(value or "media").strip().strip("/")
        return prefix or "media"

    def save(
        self,
        content: bytes,
        extension: str,
        content_type: str | None = None,
    ) -> str:
        """Save bytes and return a browser-consumable URL."""

        normalized_extension = extension if extension.startswith(".") else f".{extension}"
        media_id = f"{uuid4().hex}{normalized_extension}"
        content_type = content_type or mimetypes.guess_type(media_id)[0]

        if self.is_local:
            (self.root / media_id).write_bytes(content)
            return f"/api/media/{media_id}"

        key = f"{self._config['prefix']}/{media_id}"
        self._client_for_config().put_object(
            Bucket=self._config["bucket"],
            Key=key,
            Body=content,
            ContentType=content_type or "application/octet-stream",
        )
        return self.object_url(key)

    def save_url(self, url: str, extension: str) -> str:
        """Download a provider result and save it in the configured store."""

        if url.startswith("/api/media/"):
            return url
        parsed = urlparse(url)
        if parsed.scheme not in {"http", "https"}:
            raise ValueError("media result URL must use http or https")
        request = Request(url, headers={"User-Agent": "ai-application-factory/1.0"})
        with urlopen(request, timeout=180) as response:  # noqa: S310 - provider result URL
            content = response.read()
            content_type = response.headers.get_content_type()
        if content_type == "application/octet-stream":
            content_type = mimetypes.guess_type(f"result{extension}")[0]
        return self.save(content, extension, content_type=content_type)

    def probe_config(self, config: StorageConfig) -> StorageConfig:
        """Verify a candidate config without changing the active media backend."""

        candidate = MediaStore(self.root, client_factory=self._client_factory)
        candidate.configure(config)
        if not candidate.is_local:
            candidate._probe_remote_access()
        return dict(candidate._config)

    def _probe_remote_access(self) -> None:
        """Upload, publicly download, and clean up a remote probe object."""

        content = b"ai-application-factory-storage-probe"
        provider_label = self.provider.upper()
        try:
            url = self.save(content, ".txt", content_type="text/plain")
        except Exception as exc:
            detail = str(exc).strip() or exc.__class__.__name__
            raise ValueError(f"{provider_label} 媒体存储嗅探上传失败：{detail}") from exc

        access_error: Exception | None = None
        try:
            if not self.public_request_base_url(url):
                raise ValueError("生成的对象 URL 不是互联网可访问地址")
            request = Request(
                url,
                headers={
                    "User-Agent": "ai-application-factory/1.0",
                    "Cache-Control": "no-cache",
                },
            )
            with urlopen(request, timeout=20) as response:  # noqa: S310 - configured object URL
                downloaded = response.read()
            if downloaded != content:
                raise ValueError("下载内容与上传的探测文件不一致")
        except Exception as exc:
            access_error = exc
        finally:
            try:
                self.delete_url(url)
            except Exception as exc:  # Probe cleanup should not hide access details.
                logger.warning("媒体存储嗅探文件清理失败：%s", exc)

        if access_error is not None:
            detail = str(access_error).strip() or access_error.__class__.__name__
            raise ValueError(
                f"{provider_label} 媒体存储嗅探访问失败：{detail}。"
                "请检查 Bucket 公开读权限或公开访问域名/CDN 配置"
            ) from access_error

    @staticmethod
    def public_request_base_url(url: Any) -> str | None:
        """Return a public HTTP origin, rejecting laptop and private-network hosts."""

        value = str(url or "").strip()
        parsed = urlparse(value)
        hostname = str(parsed.hostname or "").lower().rstrip(".")
        if parsed.scheme not in {"http", "https"} or not hostname:
            return None
        if hostname == "localhost" or hostname.endswith(
            (".localhost", ".local", ".internal", ".lan")
        ):
            return None
        try:
            address = ipaddress.ip_address(hostname)
        except ValueError:
            if "." not in hostname:
                return None
        else:
            if not address.is_global:
                return None
        return f"{parsed.scheme}://{parsed.netloc}"

    def provider_reference_url(
        self, url: Any, request_base_url: Any = None
    ) -> str | None:
        """Return an externally reachable reference URL for a video-model request.

        Remote video providers cannot fetch ``/api/media/...`` from a developer's
        laptop. A local installation may opt in by setting ``public_base_url``
        (or ``PUBLIC_MEDIA_BASE_URL``) to the public API host; TOS/COS/OSS
        object URLs are already provider-reachable and pass through unchanged.
        """

        value = str(url or "").strip()
        if value.startswith(("https://", "http://")):
            return value if self.public_request_base_url(value) else None
        if not value.startswith("/api/media/") or not self.is_local:
            return None
        configured_base_url = str(
            self._config.get("public_base_url") or os.getenv("PUBLIC_MEDIA_BASE_URL") or ""
        ).strip().rstrip("/")
        base_url = configured_base_url or str(
            self.public_request_base_url(request_base_url) or ""
        )
        if not self.public_request_base_url(base_url):
            return None
        return f"{base_url}{value}" if base_url else None

    def path_for(self, media_id: str) -> Path | None:
        """Resolve a local media id safely; cloud media is served by its URL."""

        if not self.is_local:
            return None
        candidate = (self.root / media_id).resolve()
        try:
            candidate.relative_to(self.root.resolve())
        except ValueError:
            return None
        if not candidate.is_file():
            return None
        return candidate

    def delete_url(self, url: str | None) -> bool:
        """Delete a media object owned by the active store.

        Unknown/external URLs are ignored so deleting a project can never
        remove a user's unrelated remote resource.
        """

        if not isinstance(url, str) or not url:
            return False
        if self.is_local:
            if not url.startswith("/api/media/"):
                return False
            media_id = url.removeprefix("/api/media/").split("?", 1)[0]
            path = self.path_for(media_id)
            if path is None:
                return False
            path.unlink()
            return True

        key = self._key_for_url(url)
        if not key:
            return False
        self._client_for_config().delete_object(
            Bucket=self._config["bucket"],
            Key=key,
        )
        return True

    def _key_for_url(self, url: str) -> str | None:
        parsed = urlparse(url)
        if parsed.scheme not in {"http", "https"}:
            return None
        public_base_url = str(self._config.get("public_base_url") or "")
        if public_base_url and url.startswith(f"{public_base_url}/"):
            key = unquote(url[len(public_base_url) + 1 :].split("?", 1)[0])
        else:
            endpoint = urlparse(str(self._config.get("endpoint") or ""))
            bucket = str(self._config.get("bucket") or "")
            expected_host = endpoint.netloc
            if bucket and not (endpoint.hostname or "").startswith(f"{bucket}."):
                expected_host = f"{bucket}.{expected_host}"
            if not expected_host or parsed.netloc != expected_host:
                return None
            key = unquote(parsed.path.lstrip("/").split("?", 1)[0])
        prefix = self._normalize_prefix(self._config.get("prefix") or "media")
        return key if key == prefix or key.startswith(f"{prefix}/") else None

    def object_url(self, key: str) -> str:
        """Build the URL saved into project/video history for an object key."""

        encoded_key = quote(key.lstrip("/"), safe="/")
        public_base_url = str(self._config.get("public_base_url") or "")
        if public_base_url:
            return f"{public_base_url}/{encoded_key}"

        endpoint = str(self._config.get("endpoint") or "")
        parsed = urlparse(endpoint)
        if parsed.scheme and parsed.netloc:
            bucket = quote(str(self._config["bucket"]), safe="")
            host = parsed.netloc
            hostname = parsed.hostname or ""
            if bucket and not hostname.startswith(f"{self._config['bucket']}."):
                host = f"{self._config['bucket']}.{host}"
            return f"{parsed.scheme}://{host}/{encoded_key}"
        return f"{endpoint.rstrip('/')}/{quote(str(self._config['bucket']), safe='')}/{encoded_key}"

    def public_config(self) -> StorageConfig:
        """Return safe settings for API responses; never expose secret values."""

        secret_id = str(self._config.get("secret_id") or "")
        return {
            "provider": self.provider,
            "endpoint": self._config.get("endpoint", ""),
            "bucket": self._config.get("bucket", ""),
            "region": self._config.get("region", ""),
            "prefix": self._config.get("prefix", "media"),
            "public_base_url": self._config.get("public_base_url", ""),
            "secret_id_masked": self._mask_secret_id(secret_id),
            "secret_key_set": bool(self._config.get("secret_key")),
        }

    @staticmethod
    def _mask_secret_id(value: str) -> str:
        if len(value) <= 4:
            return "*" * len(value)
        return f"{value[:2]}{'*' * max(2, len(value) - 4)}{value[-2:]}"

    def _client_for_config(self) -> Any:
        if self._client is None:
            self._client = self._client_factory(
                "s3",
                endpoint_url=self._config["endpoint"],
                aws_access_key_id=self._config["secret_id"],
                aws_secret_access_key=self._config["secret_key"],
                region_name=self._config.get("region") or None,
                config=BotoConfig(
                    signature_version="s3v4",
                    s3={"addressing_style": "virtual"},
                ),
            )
        return self._client

    @staticmethod
    def content_type(path: Path) -> str:
        return mimetypes.guess_type(path.name)[0] or "application/octet-stream"


media_store = MediaStore()
