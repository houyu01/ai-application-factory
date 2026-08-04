"""Gateway layer for interactive game API use cases."""

from .game_service import InteractiveGameService, game_service


class InteractiveGameGateway:
    """Compose the interactive-game API use cases exposed to FastAPI routes."""

    def __init__(self, service: InteractiveGameService) -> None:
        self.service = service

    def create_game(self, payload, background_tasks=None):
        return self.service.create_game(payload)


game_gateway = InteractiveGameGateway(game_service)
