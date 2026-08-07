"""Documented compatibility migrations for databases created before ORM adoption.

New databases are created exclusively by ``ORMBase.metadata.create_all``. The
small amount of SQL in this module is intentionally isolated to upgrading old
SQLite files whose schemas predate the current models; application reads and
writes must use the repositories and SQLAlchemy models instead.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path


def _connection(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(str(path), timeout=30)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    return connection


def _add_missing_columns(connection: sqlite3.Connection, table: str, columns: dict[str, str]) -> None:
    existing = {row[1] for row in connection.execute(f"PRAGMA table_info({table})").fetchall()}
    for name, definition in columns.items():
        if name not in existing:
            connection.execute(f"ALTER TABLE {table} ADD COLUMN {name} {definition}")


def migrate_legacy_drama_database(path: Path) -> None:
    """Upgrade pre-ORM drama tables and fold the retired episode table into shots."""
    if str(path) == ":memory:" or not path.exists():
        return
    connection = _connection(path)
    try:
        _add_missing_columns(connection, "drama_shots", {
            "episode_id": "TEXT NOT NULL DEFAULT ''", "episode_sort_order": "INTEGER NOT NULL DEFAULT 1",
            "duration_seconds": "INTEGER NOT NULL DEFAULT 10",
            "prompt_rich_json": "TEXT NOT NULL DEFAULT '[]'", "placeholder_scene_asset_id": "TEXT",
            "placeholder_placements_json": "TEXT NOT NULL DEFAULT '[]'", "structured_json": "TEXT NOT NULL DEFAULT '{}'",
            "quality_json": "TEXT NOT NULL DEFAULT '{}'", "quality_status": "TEXT NOT NULL DEFAULT '未检查'",
            "quality_issues_json": "TEXT NOT NULL DEFAULT '[]'", "reference_asset_ids_json": "TEXT NOT NULL DEFAULT '[]'",
            "prompt_template_id": "TEXT", "prompt_template_version": "TEXT NOT NULL DEFAULT 'v1'",
        })
        _add_missing_columns(connection, "drama_assets", {
            "image_history_json": "TEXT NOT NULL DEFAULT '[]'", "content_hash": "TEXT",
            "source_type": "TEXT NOT NULL DEFAULT 'generated'", "variants_json": "TEXT NOT NULL DEFAULT '[]'",
            "metadata_json": "TEXT NOT NULL DEFAULT '{}'", "voice_id": "TEXT",
        })
        _add_missing_columns(connection, "short_dramas", {
            "video_model": "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'", "resolution": "TEXT NOT NULL DEFAULT '720p'",
            "episode_count": "INTEGER NOT NULL DEFAULT 50",
            "enable_web_search": "INTEGER NOT NULL DEFAULT 0",
            "expanded_script_min_chars": "INTEGER NOT NULL DEFAULT 50000",
            "expanded_script_max_chars": "INTEGER NOT NULL DEFAULT 100000",
            "video_public_prompt": "TEXT NOT NULL DEFAULT ''",
            "expanded_script": "TEXT NOT NULL DEFAULT ''",
            "asset_public_prompts_json": "TEXT NOT NULL DEFAULT '{}'", "shot_constraints_json": "TEXT NOT NULL DEFAULT '{}'",
        })
        _add_missing_columns(connection, "generation_tasks", {
            "job_id": "TEXT NOT NULL DEFAULT ''", "task_no": "INTEGER NOT NULL DEFAULT 1",
            "trigger_type": "TEXT NOT NULL DEFAULT 'GENERIC'", "input_snapshot_json": "TEXT",
            "output_result_json": "TEXT", "duration_ms": "INTEGER", "poll_attempts": "INTEGER NOT NULL DEFAULT 0",
            "poll_lease_token": "TEXT", "poll_lease_until": "TEXT", "provider_task_id": "TEXT",
            "progress": "INTEGER NOT NULL DEFAULT 0", "stage": "TEXT NOT NULL DEFAULT ''",
            "next_poll_at": "TEXT", "finished_at": "TEXT",
        })
        legacy = connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'drama_episodes'"
        ).fetchone()
        if legacy is not None:
            connection.execute("""
                UPDATE drama_shots
                SET episode_sort_order = COALESCE((SELECT sort_order FROM drama_episodes e
                    WHERE e.id = drama_shots.episode_id AND e.drama_id = drama_shots.drama_id), episode_sort_order, 1),
                    episode_name = COALESCE(NULLIF(episode_name, ''), (SELECT title FROM drama_episodes e
                    WHERE e.id = drama_shots.episode_id AND e.drama_id = drama_shots.drama_id), '第1集')
                WHERE EXISTS (SELECT 1 FROM drama_episodes e
                    WHERE e.id = drama_shots.episode_id AND e.drama_id = drama_shots.drama_id)
            """)
            connection.execute("DROP INDEX IF EXISTS idx_drama_episodes_drama_id")
            connection.execute("DROP TABLE drama_episodes")
        columns = {row[1] for row in connection.execute("PRAGMA table_info(short_dramas)").fetchall()}
        if "public_prompt" in columns:
            connection.execute("ALTER TABLE short_dramas DROP COLUMN public_prompt")
        if "episodes_json" in columns:
            connection.execute("ALTER TABLE short_dramas DROP COLUMN episodes_json")
        connection.commit()
    finally:
        connection.close()


def migrate_legacy_game_database(path: Path) -> None:
    """Add fields introduced by durable interactive-game task polling."""
    if str(path) == ":memory:" or not path.exists():
        return
    connection = _connection(path)
    try:
        _add_missing_columns(connection, "interactive_games", {
            "video_model": "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'",
        })
        _add_missing_columns(connection, "game_tasks", {
            "progress": "INTEGER NOT NULL DEFAULT 0", "stage": "TEXT NOT NULL DEFAULT ''",
            "poll_attempts": "INTEGER NOT NULL DEFAULT 0", "poll_lease_token": "TEXT",
            "poll_lease_until": "TEXT", "next_poll_at": "TEXT",
        })
        connection.commit()
    finally:
        connection.close()
