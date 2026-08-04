"""Short-drama persistence facade."""

from .repository_common import JSON_FIELDS, _json_dump, _json_load, _parse_datetime, utc_now
from .sqlite_repository_assets import DramaRepositoryAssetMixin
from .sqlite_repository_decomposition import DramaRepositoryDecompositionMixin
from .sqlite_repository_mapping import DramaRepositoryMappingMixin
from .sqlite_repository_projects import DramaRepositoryProjectMixin
from .sqlite_repository_settings import DramaRepositorySettingsMixin
from .sqlite_repository_setup import DramaRepositorySetupMixin
from .sqlite_repository_shots import DramaRepositoryShotMixin
from .sqlite_repository_tasks import DramaRepositoryTaskMixin


class SQLiteRepository(
    DramaRepositorySetupMixin,
    DramaRepositorySettingsMixin,
    DramaRepositoryMappingMixin,
    DramaRepositoryProjectMixin,
    DramaRepositoryDecompositionMixin,
    DramaRepositoryTaskMixin,
    DramaRepositoryAssetMixin,
    DramaRepositoryShotMixin,
):
    """Compatibility facade for all short-drama repository operations."""
