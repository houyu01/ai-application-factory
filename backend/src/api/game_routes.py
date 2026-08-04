"""Interactive-game endpoints used by the game console and runtime client."""

from fastapi import BackgroundTasks, HTTPException

from ..domain.models import (
    GameChoiceRequest,
    GameEdgeCreate,
    GameEdgeUpdate,
    GameNodeUpdate,
    InteractiveGameCreate,
    ModelSelectionUpdate,
    TaskResponse,
)
from .router_common import api_router, game_gateway, game_service


@api_router.get("/games")
def list_games():
    """Frontend game tab calls this when it becomes active to load project cards."""

    return game_service.list_games()


@api_router.post("/games", status_code=202)
def create_game(payload: InteractiveGameCreate, background_tasks: BackgroundTasks):
    """Game creation form calls this after the user submits a valid script and configuration."""

    return game_gateway.create_game(payload, background_tasks)


@api_router.get("/games/{game_id}")
def get_game(game_id: str):
    """The game editor calls this when opening a selected game project."""

    try:
        return game_service.get_game(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/games/{game_id}")
def delete_game(game_id: str):
    """The game list calls this after the user confirms deleting a complete game graph."""

    try:
        return game_service.delete_game(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/games/{game_id}/models")
def update_game_models(game_id: str, payload: ModelSelectionUpdate):
    """The game global-parameters form calls this when model selections are saved."""

    try:
        return game_service.repository.update_model_selection(
            game_id, payload.model_dump(exclude_none=True)
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@api_router.get("/games/{game_id}/runtime-manifest")
def get_game_runtime_manifest(game_id: str):
    """The exported game runtime calls this to obtain nodes, edges, engine, and start node."""

    try:
        game = game_service.get_game(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    start_node = next(
        (node for node in game.get("nodes", []) if node["node_type"] == "start"),
        None,
    )
    return {
        "game_id": game["id"],
        "name": game["name"],
        "platform": game["platform"],
        "engine": "Unity" if game["platform"] == "Steam游戏" else "Cocos Creator",
        "start_node_id": start_node["id"] if start_node else None,
        "nodes": game.get("nodes", []),
        "edges": game.get("edges", []),
    }


@api_router.post("/games/{game_id}/sessions", status_code=201)
def create_game_session(game_id: str):
    """A game client calls this when a player starts a new playable session."""

    try:
        return game_service.create_session(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.get("/games/{game_id}/sessions/{session_id}")
def get_game_session(game_id: str, session_id: str):
    """A game client calls this after loading or refreshing a player's current path."""

    try:
        return game_service.get_session(game_id, session_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/games/{game_id}/sessions/{session_id}/choices")
def choose_game_session_edge(
    game_id: str, session_id: str, payload: GameChoiceRequest
):
    """A game client calls this when the player selects a visible branch option."""

    try:
        return game_service.choose_session_edge(game_id, session_id, payload.edge_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.post("/games/{game_id}/nodes/{node_id}/video", response_model=TaskResponse, status_code=202)
def generate_game_node_video(
    game_id: str, node_id: str, background_tasks: BackgroundTasks
):
    """The game editor calls this when the user requests a node video generation task."""

    try:
        task = game_service.enqueue_node_video(game_id, node_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return {
        "id": task["id"],
        "type": task["type"],
        "status": task["status"],
        "project_id": game_id,
        "created_at": task["created_at"],
        "resource_id": node_id,
        "started_at": task.get("started_at"),
        "completed_at": task.get("completed_at"),
        "error_message": task.get("error_message"),
    }


@api_router.put("/games/{game_id}/nodes/{node_id}")
def update_game_node(game_id: str, node_id: str, payload: GameNodeUpdate):
    """The node editor calls this after the user changes a branch node's fields."""

    try:
        return game_service.update_node(game_id, node_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/games/{game_id}/edges")
def create_game_edge(game_id: str, payload: GameEdgeCreate):
    """The graph editor calls this when the user creates a choice edge."""

    try:
        return game_service.create_edge(game_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/games/{game_id}/edges/{edge_id}")
def update_game_edge(game_id: str, edge_id: str, payload: GameEdgeUpdate):
    """The graph editor calls this when the user edits an existing choice edge."""

    try:
        return game_service.update_edge(game_id, edge_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/games/{game_id}/edges/{edge_id}", status_code=204)
def delete_game_edge(game_id: str, edge_id: str):
    """The graph editor calls this after the user removes a choice edge."""

    try:
        game_service.delete_edge(game_id, edge_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/game-tasks/{task_id}")
def get_game_task(task_id: str):
    """The game editor polls this after requesting a node video to restore its loading state."""

    try:
        return game_service.get_task(task_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
