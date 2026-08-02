"""SQLite persistence for short-drama projects and generation tasks.

The repository deliberately uses the standard library so local development has
no extra database service or ORM to configure. JSON columns keep the API
payloads flexible while the shot and asset tables preserve independently
addressable resources.
"""

from __future__ import annotations

import json
import os
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator
from uuid import uuid4

from ..domain.models import GenerationStatus, ProjectCreate


JSON_FIELDS = {
    "shots_json": "shots",
    "assets_json": "assets",
    "historical_videos_json": "historical_videos",
    "result_json": "result",
}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _parse_datetime(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None


def _json_load(value: str | None, default: Any) -> Any:
    if not value:
        return default
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return default


def _json_dump(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False)


class SQLiteRepository:
    """Repository with one short-lived SQLite connection per operation."""

    def __init__(self, database_path: str | Path | None = None) -> None:
        default_path = Path(__file__).resolve().parents[2] / "data" / "ai_application_factory.db"
        configured_path = database_path or os.getenv("DATABASE_PATH") or default_path
        self.database_path = Path(configured_path)
        if str(self.database_path) != ":memory:":
            self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self._initialize()

    @contextmanager
    def _connect(self) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(str(self.database_path), timeout=30)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        try:
            yield connection
            connection.commit()
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def _initialize(self) -> None:
        with self._connect() as connection:
            connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS short_dramas (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    script TEXT NOT NULL,
                    ratio TEXT NOT NULL,
                    style TEXT NOT NULL,
                    theme TEXT NOT NULL,
                    language_model TEXT NOT NULL,
                    multimodal_model TEXT NOT NULL,
                    video_model TEXT NOT NULL DEFAULT 'doubao-seedance-2.0',
                    status TEXT NOT NULL,
                    shots_json TEXT NOT NULL DEFAULT '[]',
                    assets_json TEXT NOT NULL DEFAULT '[]',
                    historical_videos_json TEXT NOT NULL DEFAULT '[]',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS drama_assets (
                    id TEXT PRIMARY KEY,
                    drama_id TEXT NOT NULL,
                    type TEXT NOT NULL,
                    name TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    image_url TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS drama_shots (
                    id TEXT PRIMARY KEY,
                    drama_id TEXT NOT NULL,
                    episode_id TEXT NOT NULL,
                    episode_name TEXT NOT NULL,
                    episode_sort_order INTEGER NOT NULL DEFAULT 1,
                    shot_index INTEGER NOT NULL,
                    title TEXT NOT NULL,
                    original_text TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    status TEXT NOT NULL,
                    historical_videos_json TEXT NOT NULL DEFAULT '[]',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS generation_tasks (
                    id TEXT PRIMARY KEY,
                    drama_id TEXT NOT NULL,
                    type TEXT NOT NULL,
                    job_id TEXT NOT NULL DEFAULT '',
                    task_no INTEGER NOT NULL DEFAULT 1,
                    trigger_type TEXT NOT NULL DEFAULT 'GENERIC',
                    resource_id TEXT,
                    status TEXT NOT NULL,
                    input_snapshot_json TEXT,
                    output_result_json TEXT,
                    result_json TEXT,
                    error_message TEXT,
                    duration_ms INTEGER,
                    poll_attempts INTEGER NOT NULL DEFAULT 0,
                    poll_lease_token TEXT,
                    poll_lease_until TEXT,
                    created_at TEXT NOT NULL,
                    started_at TEXT,
                    finished_at TEXT,
                    completed_at TEXT,
                    FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_drama_assets_drama_id ON drama_assets(drama_id);
                CREATE INDEX IF NOT EXISTS idx_drama_shots_drama_id ON drama_shots(drama_id);
                CREATE INDEX IF NOT EXISTS idx_generation_tasks_drama_id ON generation_tasks(drama_id);
                """
            )
            self._ensure_optional_columns(
                connection,
                "drama_shots",
                {
                    "episode_id": "TEXT NOT NULL DEFAULT ''",
                    "episode_sort_order": "INTEGER NOT NULL DEFAULT 1",
                },
            )
            self._ensure_optional_columns(
                connection,
                "short_dramas",
                {"video_model": "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'"},
            )
            self._ensure_optional_columns(
                connection,
                "generation_tasks",
                {
                    "job_id": "TEXT NOT NULL DEFAULT ''",
                    "task_no": "INTEGER NOT NULL DEFAULT 1",
                    "trigger_type": "TEXT NOT NULL DEFAULT 'GENERIC'",
                    "input_snapshot_json": "TEXT",
                    "output_result_json": "TEXT",
                    "duration_ms": "INTEGER",
                    "poll_attempts": "INTEGER NOT NULL DEFAULT 0",
                    "poll_lease_token": "TEXT",
                    "poll_lease_until": "TEXT",
                    "finished_at": "TEXT",
                },
            )
            self._migrate_legacy_episodes(connection)
            self._remove_legacy_episode_snapshot(connection)

    @staticmethod
    def _ensure_optional_columns(
        connection: sqlite3.Connection,
        table: str,
        columns: dict[str, str],
    ) -> None:
        existing = {
            row[1]
            for row in connection.execute(f"PRAGMA table_info({table})").fetchall()
        }
        for name, definition in columns.items():
            if name not in existing:
                connection.execute(f"ALTER TABLE {table} ADD COLUMN {name} {definition}")

    @staticmethod
    def _migrate_legacy_episodes(connection: sqlite3.Connection) -> None:
        """Move episode metadata onto shots, then remove the redundant table."""

        legacy_table = connection.execute(
            """
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = 'drama_episodes'
            """
        ).fetchone()
        if legacy_table is None:
            return

        connection.execute(
            """
            UPDATE drama_shots
            SET episode_sort_order = COALESCE(
                    (
                        SELECT episode.sort_order
                        FROM drama_episodes AS episode
                        WHERE episode.id = drama_shots.episode_id
                          AND episode.drama_id = drama_shots.drama_id
                    ),
                    episode_sort_order,
                    1
                ),
                episode_name = COALESCE(
                    NULLIF(episode_name, ''),
                    (
                        SELECT episode.title
                        FROM drama_episodes AS episode
                        WHERE episode.id = drama_shots.episode_id
                          AND episode.drama_id = drama_shots.drama_id
                    ),
                    '第1集'
                )
            WHERE EXISTS (
                SELECT 1
                FROM drama_episodes AS episode
                WHERE episode.id = drama_shots.episode_id
                  AND episode.drama_id = drama_shots.drama_id
            )
            """
        )
        connection.execute("DROP INDEX IF EXISTS idx_drama_episodes_drama_id")
        connection.execute("DROP TABLE drama_episodes")

    @staticmethod
    def _remove_legacy_episode_snapshot(connection: sqlite3.Connection) -> None:
        """Remove the old duplicated episode JSON snapshot when upgrading a DB."""

        columns = {
            row[1]
            for row in connection.execute("PRAGMA table_info(short_dramas)").fetchall()
        }
        if "episodes_json" in columns:
            connection.execute("ALTER TABLE short_dramas DROP COLUMN episodes_json")

    @staticmethod
    def _drama_from_row(row: sqlite3.Row) -> dict[str, Any]:
        drama = dict(row)
        for column, output_key in JSON_FIELDS.items():
            if column in drama:
                drama[output_key] = _json_load(drama.pop(column), [])
        drama.setdefault("episodes", [])
        drama.setdefault("shots", [])
        drama.setdefault("assets", [])
        drama.setdefault("historical_videos", [])
        return drama

    @staticmethod
    def _asset_from_row(row: sqlite3.Row) -> dict[str, Any]:
        return dict(row)

    @staticmethod
    def _shot_from_row(row: sqlite3.Row) -> dict[str, Any]:
        shot = dict(row)
        shot["historical_videos"] = _json_load(shot.pop("historical_videos_json"), [])
        return shot

    @staticmethod
    def _aggregate_episodes(shots: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Build the public episode view from the episode fields on each shot."""

        grouped: dict[str, dict[str, Any]] = {}
        for shot in shots:
            episode_id = str(shot.get("episode_id") or "episode:1")
            raw_sort_order = shot.get("episode_sort_order", 1)
            try:
                sort_order = max(1, int(raw_sort_order))
            except (TypeError, ValueError):
                sort_order = 1
            episode = grouped.setdefault(
                episode_id,
                {
                    "id": episode_id,
                    "sort_order": sort_order,
                    "title": shot.get("episode_name") or f"第{sort_order}集",
                    "shot_count": 0,
                },
            )
            episode["shot_count"] += 1

        return sorted(
            grouped.values(),
            key=lambda episode: (episode["sort_order"], episode["id"]),
        )

    @staticmethod
    def _task_from_row(row: sqlite3.Row) -> dict[str, Any]:
        task = dict(row)
        # The persistence schema calls this foreign key ``drama_id`` while
        # the public API uses the provider-neutral ``project_id`` name.
        task["project_id"] = task["drama_id"]
        output_value = task.pop("output_result_json", None)
        legacy_result_value = task.pop("result_json", None)
        result_value = output_value or legacy_result_value
        input_value = task.pop("input_snapshot_json", None)
        task["input_snapshot"] = _json_load(input_value, None)
        task["result"] = _json_load(result_value, None)
        return task

    def create_drama_with_task(self, payload: ProjectCreate) -> tuple[dict[str, Any], dict[str, Any]]:
        drama_id = str(uuid4())
        task_id = str(uuid4())
        timestamp = utc_now()
        task_type = "script_decomposition"
        values = payload.model_dump()

        with self._connect() as connection:
            connection.execute(
                """
                INSERT INTO short_dramas (
                    id, name, script, ratio, style, theme, language_model,
                    multimodal_model, video_model, status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    drama_id,
                    values["name"],
                    values["script"],
                    values["ratio"],
                    values["style"],
                    values["theme"],
                    values["language_model"],
                    values["multimodal_model"],
                    values["video_model"],
                    GenerationStatus.NOT_GENERATED.value,
                    timestamp,
                    timestamp,
                ),
            )
            connection.execute(
                """
                INSERT INTO generation_tasks (
                    id, drama_id, type, job_id, task_no, trigger_type,
                    status, input_snapshot_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    task_id,
                    drama_id,
                    task_type,
                    drama_id,
                    1,
                    "DRAMA_BOOTSTRAP",
                    GenerationStatus.NOT_GENERATED.value,
                    _json_dump(
                        {
                            "drama_id": drama_id,
                            "script": values["script"],
                            "language_model": values["language_model"],
                        }
                    ),
                    timestamp,
                ),
            )
            drama_row = connection.execute(
                "SELECT * FROM short_dramas WHERE id = ?", (drama_id,)
            ).fetchone()
            task_row = connection.execute(
                "SELECT * FROM generation_tasks WHERE id = ?", (task_id,)
            ).fetchone()

        assert drama_row is not None
        assert task_row is not None
        return self._drama_from_row(drama_row), self._task_from_row(task_row)

    def list_dramas(self) -> list[dict[str, Any]]:
        with self._connect() as connection:
            drama_rows = connection.execute(
                "SELECT * FROM short_dramas ORDER BY created_at DESC"
            ).fetchall()
            shot_rows = connection.execute(
                """
                SELECT * FROM drama_shots
                ORDER BY drama_id, episode_sort_order, episode_name, shot_index, created_at
                """
            ).fetchall()
            asset_rows = connection.execute(
                "SELECT * FROM drama_assets ORDER BY drama_id, created_at, id"
            ).fetchall()

        shots_by_drama: dict[str, list[dict[str, Any]]] = {}
        for row in shot_rows:
            shot = self._shot_from_row(row)
            shots_by_drama.setdefault(shot["drama_id"], []).append(shot)
        assets_by_drama: dict[str, list[dict[str, Any]]] = {}
        for row in asset_rows:
            asset = self._asset_from_row(row)
            assets_by_drama.setdefault(asset["drama_id"], []).append(asset)

        projects: list[dict[str, Any]] = []
        for row in drama_rows:
            drama = self._drama_from_row(row)
            shots = shots_by_drama.get(drama["id"], [])
            assets = assets_by_drama.get(drama["id"], [])
            drama["shots"] = shots
            drama["assets"] = assets
            drama["episodes"] = self._aggregate_episodes(shots)
            projects.append(drama)
        return projects

    def get_drama(self, drama_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            drama_row = connection.execute(
                "SELECT * FROM short_dramas WHERE id = ?", (drama_id,)
            ).fetchone()
            if drama_row is None:
                return None
            assets = connection.execute(
                "SELECT * FROM drama_assets WHERE drama_id = ? ORDER BY created_at, id",
                (drama_id,),
            ).fetchall()
            shots = connection.execute(
                """
                SELECT * FROM drama_shots
                WHERE drama_id = ?
                ORDER BY episode_sort_order, episode_name, shot_index, created_at
                """,
                (drama_id,),
            ).fetchall()
            tasks = connection.execute(
                "SELECT * FROM generation_tasks WHERE drama_id = ? ORDER BY created_at",
                (drama_id,),
            ).fetchall()

        drama = self._drama_from_row(drama_row)
        drama["assets"] = [self._asset_from_row(row) for row in assets]
        drama["shots"] = [self._shot_from_row(row) for row in shots]
        drama["episodes"] = self._aggregate_episodes(drama["shots"])
        drama["tasks"] = [self._task_from_row(row) for row in tasks]
        return drama

    def set_drama_status(self, drama_id: str, status: GenerationStatus) -> None:
        with self._connect() as connection:
            connection.execute(
                "UPDATE short_dramas SET status = ?, updated_at = ? WHERE id = ?",
                (status.value, utc_now(), drama_id),
            )

    def save_decomposition(
        self,
        drama_id: str,
        episodes: list[dict[str, Any]],
        shots: list[dict[str, Any]],
        assets: list[dict[str, Any]],
    ) -> None:
        timestamp = utc_now()
        normalized_episodes = []
        normalized_assets = []
        normalized_shots = []
        valid_statuses = {status.value for status in GenerationStatus}

        for episode_index, episode in enumerate(episodes, start=1):
            raw_episode_id = episode.get("id") or str(uuid4())
            normalized_episodes.append(
                {
                    "id": f"{drama_id}:episode:{raw_episode_id}:{episode_index}",
                    "sort_order": episode_index,
                    "title": episode.get("title", episode.get("name", f"第{episode_index}集")),
                }
            )

        for asset_index, asset in enumerate(assets, start=1):
            raw_asset_id = asset.get("id") or str(uuid4())
            asset_status = asset.get("status", GenerationStatus.NOT_GENERATED.value)
            if asset_status not in valid_statuses:
                asset_status = GenerationStatus.NOT_GENERATED.value
            normalized_assets.append(
                {
                    # Planner-provided IDs are only unique inside one plan.
                    "id": f"{drama_id}:asset:{raw_asset_id}:{asset_index}",
                    "type": asset.get("type", "prop"),
                    "name": asset.get("name", "未命名元素"),
                    "prompt": asset.get("prompt", ""),
                    "image_url": asset.get("image_url"),
                    "status": asset_status,
                }
            )

        for index, shot in enumerate(shots, start=1):
            raw_shot_id = shot.get("id") or str(uuid4())
            raw_episode_index = shot.get("episode_index", 1)
            try:
                episode_index = max(1, int(raw_episode_index))
            except (TypeError, ValueError):
                episode_index = 1
            if normalized_episodes:
                episode_index = min(episode_index, len(normalized_episodes))
                episode = normalized_episodes[episode_index - 1]
            else:
                episode = {
                    "id": f"{drama_id}:episode:1",
                    "sort_order": 1,
                    "title": "第1集",
                }
            normalized_shots.append(
                {
                    # Prefix IDs so two dramas can safely use shot_001, etc.
                    "id": f"{drama_id}:shot:{raw_shot_id}:{index}",
                    "episode_id": episode["id"],
                    "episode_name": episode["title"],
                    "episode_sort_order": episode["sort_order"],
                    "shot_index": shot.get("shot_index", index),
                    "title": shot.get("title", f"分镜 {index}"),
                    "original_text": shot.get("original_text", shot.get("script", "")),
                    "prompt": shot.get("prompt", ""),
                    "status": shot.get("status", GenerationStatus.NOT_GENERATED.value),
                    "historical_videos": shot.get("historical_videos", []),
                }
            )

        with self._connect() as connection:
            connection.execute("DELETE FROM drama_assets WHERE drama_id = ?", (drama_id,))
            connection.execute("DELETE FROM drama_shots WHERE drama_id = ?", (drama_id,))
            connection.executemany(
                """
                INSERT INTO drama_assets (
                    id, drama_id, type, name, prompt, image_url, status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    (
                        asset["id"],
                        drama_id,
                        asset["type"],
                        asset["name"],
                        asset["prompt"],
                        asset["image_url"],
                        asset["status"],
                        timestamp,
                        timestamp,
                    )
                    for asset in normalized_assets
                ],
            )
            connection.executemany(
                """
                INSERT INTO drama_shots (
                    id, drama_id, episode_id, episode_name, episode_sort_order, shot_index,
                    title, original_text, prompt, status, historical_videos_json,
                    created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    (
                        shot["id"],
                        drama_id,
                        shot["episode_id"],
                        shot["episode_name"],
                        shot["episode_sort_order"],
                        shot["shot_index"],
                        shot["title"],
                        shot["original_text"],
                        shot["prompt"],
                        shot["status"],
                        _json_dump(shot["historical_videos"]),
                        timestamp,
                        timestamp,
                    )
                    for shot in normalized_shots
                ],
            )
            connection.execute(
                """
                UPDATE short_dramas
                SET shots_json = ?, assets_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    _json_dump(normalized_shots),
                    _json_dump(normalized_assets),
                    timestamp,
                    drama_id,
                ),
            )

    def create_task(
        self,
        drama_id: str,
        task_type: str,
        resource_id: str | None = None,
        input_snapshot: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        task_id = str(uuid4())
        timestamp = utc_now()
        job_id = f"{drama_id}:{resource_id or task_type}"
        trigger_type = {
            "script_decomposition": "DRAMA_BOOTSTRAP",
            "asset_image": "DRAMA_ASSET",
            "shot_prompt": "DRAMA_SHOT_PROMPT",
            "shot_video": "DRAMA_VIDEO",
        }.get(task_type, task_type.upper())
        with self._connect() as connection:
            task_no_row = connection.execute(
                "SELECT COALESCE(MAX(task_no), 0) + 1 FROM generation_tasks WHERE job_id = ?",
                (job_id,),
            ).fetchone()
            task_no = int(task_no_row[0]) if task_no_row else 1
            connection.execute(
                """
                INSERT INTO generation_tasks (
                    id, drama_id, type, job_id, task_no, trigger_type, resource_id,
                    status, input_snapshot_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    task_id,
                    drama_id,
                    task_type,
                    job_id,
                    task_no,
                    trigger_type,
                    resource_id,
                    GenerationStatus.NOT_GENERATED.value,
                    _json_dump(input_snapshot) if input_snapshot is not None else None,
                    timestamp,
                ),
            )
            row = connection.execute(
                "SELECT * FROM generation_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        assert row is not None
        return self._task_from_row(row)

    def get_task(self, task_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM generation_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        return self._task_from_row(row) if row else None

    def get_asset(self, drama_id: str, asset_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM drama_assets WHERE drama_id = ? AND id = ?",
                (drama_id, asset_id),
            ).fetchone()
        return self._asset_from_row(row) if row else None

    def get_shot(self, drama_id: str, shot_id: str) -> dict[str, Any] | None:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM drama_shots WHERE drama_id = ? AND id = ?",
                (drama_id, shot_id),
            ).fetchone()
        return self._shot_from_row(row) if row else None

    def update_task_status(
        self,
        task_id: str,
        status: GenerationStatus,
        *,
        result: dict[str, Any] | None = None,
        error_message: str | None = None,
    ) -> dict[str, Any]:
        with self._connect() as connection:
            existing = connection.execute(
                "SELECT * FROM generation_tasks WHERE id = ?", (task_id,)
            ).fetchone()
            if existing is None:
                raise KeyError(f"Task not found: {task_id}")

            started_at = existing["started_at"]
            completed_at = existing["completed_at"]
            finished_at = existing["finished_at"]
            if status is GenerationStatus.GENERATING and started_at is None:
                started_at = utc_now()
            if status in (GenerationStatus.SUCCEEDED, GenerationStatus.FAILED):
                completed_at = utc_now()
                finished_at = completed_at
            started_timestamp = _parse_datetime(started_at)
            finished_timestamp = _parse_datetime(finished_at)
            duration_ms = existing["duration_ms"]
            if started_timestamp and finished_timestamp:
                duration_ms = max(
                    0,
                    int((finished_timestamp - started_timestamp).total_seconds() * 1000),
                )
            connection.execute(
                """
                UPDATE generation_tasks
                SET status = ?, result_json = ?, output_result_json = ?, error_message = ?,
                    started_at = ?, finished_at = ?, completed_at = ?, duration_ms = ?
                WHERE id = ?
                """,
                (
                    status.value,
                    _json_dump(result) if result is not None else existing["result_json"],
                    _json_dump(result)
                    if result is not None
                    else existing["output_result_json"],
                    error_message,
                    started_at,
                    finished_at,
                    completed_at,
                    duration_ms,
                    task_id,
                ),
            )
            row = connection.execute(
                "SELECT * FROM generation_tasks WHERE id = ?", (task_id,)
            ).fetchone()
        assert row is not None
        return self._task_from_row(row)

    def update_asset_status(
        self, asset_id: str, status: GenerationStatus, image_url: str | None = None
    ) -> None:
        with self._connect() as connection:
            connection.execute(
                """
                UPDATE drama_assets
                SET status = ?, image_url = COALESCE(?, image_url), updated_at = ?
                WHERE id = ?
                """,
                (status.value, image_url, utc_now(), asset_id),
            )

    def update_asset(
        self,
        drama_id: str,
        asset_id: str,
        *,
        name: str | None = None,
        prompt: str | None = None,
        image_url: str | None = None,
    ) -> dict[str, Any]:
        with self._connect() as connection:
            connection.execute(
                """
                UPDATE drama_assets
                SET name = COALESCE(?, name), prompt = COALESCE(?, prompt),
                    image_url = COALESCE(?, image_url), updated_at = ?
                WHERE drama_id = ? AND id = ?
                """,
                (name, prompt, image_url, utc_now(), drama_id, asset_id),
            )
            row = connection.execute(
                "SELECT * FROM drama_assets WHERE drama_id = ? AND id = ?",
                (drama_id, asset_id),
            ).fetchone()
        if row is None:
            raise KeyError(f"Asset not found: {asset_id}")
        return self._asset_from_row(row)

    def update_shot(
        self,
        drama_id: str,
        shot_id: str,
        *,
        title: str | None = None,
        original_text: str | None = None,
        prompt: str | None = None,
        status: GenerationStatus | None = None,
    ) -> dict[str, Any]:
        with self._connect() as connection:
            connection.execute(
                """
                UPDATE drama_shots
                SET title = COALESCE(?, title), original_text = COALESCE(?, original_text),
                    prompt = COALESCE(?, prompt), status = COALESCE(?, status), updated_at = ?
                WHERE drama_id = ? AND id = ?
                """,
                (
                    title,
                    original_text,
                    prompt,
                    status.value if status else None,
                    utc_now(),
                    drama_id,
                    shot_id,
                ),
            )
            row = connection.execute(
                "SELECT * FROM drama_shots WHERE drama_id = ? AND id = ?",
                (drama_id, shot_id),
            ).fetchone()
        if row is None:
            raise KeyError(f"Shot not found: {shot_id}")
        return self._shot_from_row(row)

    def add_historical_video(
        self,
        drama_id: str,
        shot_id: str,
        video: dict[str, Any],
    ) -> dict[str, Any]:
        with self._connect() as connection:
            row = connection.execute(
                """
                SELECT historical_videos_json FROM drama_shots
                WHERE id = ? AND drama_id = ?
                """,
                (shot_id, drama_id),
            ).fetchone()
            if row is None:
                raise KeyError(f"Shot not found: {shot_id}")

            history = _json_load(row["historical_videos_json"], [])
            history.append(video)
            timestamp = utc_now()
            connection.execute(
                """
                UPDATE drama_shots
                SET historical_videos_json = ?, status = ?, updated_at = ?
                WHERE id = ? AND drama_id = ?
                """,
                (
                    _json_dump(history),
                    GenerationStatus.SUCCEEDED.value,
                    timestamp,
                    shot_id,
                    drama_id,
                ),
            )

            drama_row = connection.execute(
                "SELECT historical_videos_json FROM short_dramas WHERE id = ?", (drama_id,)
            ).fetchone()
            drama_history = _json_load(drama_row["historical_videos_json"], []) if drama_row else []
            drama_history.append({**video, "shot_id": shot_id})
            connection.execute(
                """
                UPDATE short_dramas
                SET historical_videos_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (_json_dump(drama_history), timestamp, drama_id),
            )
        return video
