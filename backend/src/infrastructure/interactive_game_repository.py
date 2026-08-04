"""Interactive-game persistence facade."""

from .game_repository_graph import GameRepositoryGraphMixin
from .game_repository_mapping import GameRepositoryMappingMixin
from .game_repository_runtime import GameRepositoryRuntimeMixin
from .game_repository_setup import GameRepositorySetupMixin
from .game_repository_tasks import GameRepositoryTaskMixin


class InteractiveGameRepository(
    GameRepositorySetupMixin,
    GameRepositoryMappingMixin,
    GameRepositoryGraphMixin,
    GameRepositoryTaskMixin,
    GameRepositoryRuntimeMixin,
):
    """Compatibility facade for all interactive-game repository operations."""
