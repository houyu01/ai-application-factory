"""SQLAlchemy engine and transaction boundary for local and deployed storage."""

from __future__ import annotations

import os
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

from sqlalchemy import create_engine, event
from sqlalchemy.engine import Engine
from sqlalchemy.orm import Session, sessionmaker

from .orm_models import ORMBase


class Database:
    """Owns the SQLAlchemy engine and sessions used by repositories.

    Application services call repositories, while repositories use this class
    to create short-lived transactions. Centralizing it prevents connection
    setup, SQLite pragmas, and future cloud-database changes from leaking into
    business code.
    """

    def __init__(self, database_path: str | Path | None = None) -> None:
        default_path = Path(__file__).resolve().parents[2] / "data" / "ai_application_factory.db"
        configured = database_path or os.getenv("DATABASE_PATH") or default_path
        self.database_path = Path(configured)
        if str(self.database_path) != ":memory:":
            self.database_path.parent.mkdir(parents=True, exist_ok=True)
        url = "sqlite:///:memory:" if str(self.database_path) == ":memory:" else f"sqlite:///{self.database_path}"
        self.engine = create_engine(
            url,
            connect_args={"check_same_thread": False, "timeout": 30},
            future=True,
        )
        self.session_factory = sessionmaker(bind=self.engine, expire_on_commit=False, class_=Session)
        ORMBase.metadata.create_all(self.engine)

    @contextmanager
    def session(self) -> Iterator[Session]:
        """Yield one ORM transaction and roll it back if the operation fails."""

        session = self.session_factory()
        try:
            yield session
            session.commit()
        except Exception:
            session.rollback()
            raise
        finally:
            session.close()


@event.listens_for(Engine, "connect")
def _enable_sqlite_foreign_keys(dbapi_connection, _connection_record) -> None:
    """Keep SQLite foreign-key cascades enabled for every ORM connection."""

    if dbapi_connection.__class__.__module__.startswith("sqlite3"):
        cursor = dbapi_connection.cursor()
        cursor.execute("PRAGMA foreign_keys=ON")
        cursor.close()
