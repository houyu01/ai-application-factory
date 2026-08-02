"""Gateway layer for interactive game API use cases."""

from fastapi import BackgroundTasks

from .game_service import InteractiveGameService, game_service


class InteractiveGameGateway:
    def __init__(self, service: InteractiveGameService) -> None:
        self.service = service

    def create_game(self, payload, background_tasks: BackgroundTasks):
        game = self.service.create_game(payload)
        background_tasks.add_task(
            self.service.decompose_game, game["task_id"], game["id"]
        )
        return game


game_gateway = InteractiveGameGateway(game_service)
