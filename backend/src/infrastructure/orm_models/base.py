"""Shared SQLAlchemy declarative base for the application database."""

from sqlalchemy.orm import DeclarativeBase


class ORMBase(DeclarativeBase):
    """Base class used by every persisted product asset model."""

