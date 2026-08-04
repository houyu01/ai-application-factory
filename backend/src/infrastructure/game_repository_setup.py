"""Interactive-game repository initialization."""

from __future__ import annotations

import os
from pathlib import Path

from .database import Database
from .legacy_sqlite_migrations import migrate_legacy_game_database


class GameRepositorySetupMixin:
    """Initialize ORM-backed interactive-game storage at application startup.

    The game list/editor and runtime services use the repository after this
    class has created all tables. Only the isolated legacy migration handles
    databases created before the current task-progress columns existed.
    """

    def __init__(self, database_path: str | Path | None = None) -> None:
        default_path = Path(__file__).resolve().parents[2] / "data" / "ai_application_factory.db"
        configured_path = database_path or os.getenv("DATABASE_PATH") or default_path
        self.database_path = Path(configured_path)
        if str(self.database_path) != ":memory:":
            self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self.database = Database(self.database_path)
        migrate_legacy_game_database(self.database_path)
