from fastapi import APIRouter, BackgroundTasks, HTTPException
from pydantic import BaseModel, Field
from ..application.game_gateway import game_gateway
from ..application.game_service import game_service
from ..application.drama_gateway import drama_gateway
from ..application.task_service import task_service
from ..domain.models import (
    GameEdgeCreate,
    GameEdgeUpdate,
    GameChoiceRequest,
    GameNodeUpdate,
    InteractiveGameCreate,
    DramaAssetUpdate,
    DramaShotUpdate,
    DramaVideoGenerationRequest,
    ProjectCreate,
    TaskResponse,
)

api_router = APIRouter()

class ModelConfig(BaseModel):
    kind: str = Field(pattern="^(language|multimodal)$")
    model: str
    endpoint: str
    api_key: str

@api_router.get("/projects")
def list_projects():
    return task_service.list_projects()

@api_router.post("/projects", status_code=202)
def create_project(payload: ProjectCreate, background_tasks: BackgroundTasks):
    return drama_gateway.create_project(payload, background_tasks)

@api_router.get("/projects/{project_id}")
def get_project(project_id: str):
    try:
        return task_service.get_project(project_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

@api_router.post("/projects/{project_id}/assets/{asset_id}/image", response_model=TaskResponse, status_code=202)
def generate_asset_image(project_id: str, asset_id: str, background_tasks: BackgroundTasks):
    try:
        task = task_service.enqueue("asset_image", project_id, asset_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    background_tasks.add_task(task_service.run_asset_image, task["id"], project_id, asset_id)
    return task


@api_router.put("/projects/{project_id}/assets/{asset_id}")
def update_project_asset(project_id: str, asset_id: str, payload: DramaAssetUpdate):
    try:
        return task_service.repository.update_asset(
            project_id,
            asset_id,
            name=payload.name,
            prompt=payload.prompt,
            image_url=payload.image_url,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/projects/{project_id}/shots/{shot_id}/prompt", response_model=TaskResponse, status_code=202)
def generate_shot_prompt(project_id: str, shot_id: str, background_tasks: BackgroundTasks):
    try:
        task = task_service.enqueue("shot_prompt", project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    background_tasks.add_task(task_service.run_shot_prompt, task["id"], project_id, shot_id)
    return task


@api_router.put("/projects/{project_id}/shots/{shot_id}")
def update_project_shot(project_id: str, shot_id: str, payload: DramaShotUpdate):
    try:
        return task_service.repository.update_shot(
            project_id,
            shot_id,
            title=payload.title,
            original_text=payload.original_text,
            prompt=payload.prompt,
        )
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

@api_router.post("/projects/{project_id}/shots/{shot_id}/video", response_model=TaskResponse, status_code=202)
def generate_shot_video(
    project_id: str,
    shot_id: str,
    background_tasks: BackgroundTasks,
    payload: DramaVideoGenerationRequest | None = None,
):
    try:
        task = task_service.enqueue("shot_video", project_id, shot_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    background_tasks.add_task(
        task_service.run_shot_video,
        task["id"],
        project_id,
        shot_id,
        payload.video_url if payload else None,
    )
    return task

@api_router.get("/tasks/{task_id}")
def get_task(task_id: str):
    try:
        return task_service.get_task(task_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

@api_router.put("/settings/models")
def save_model_config(config: ModelConfig):
    return task_service.save_model_config(config.model_dump())


@api_router.get("/games")
def list_games():
    return game_service.list_games()


@api_router.post("/games", status_code=202)
def create_game(payload: InteractiveGameCreate, background_tasks: BackgroundTasks):
    return game_gateway.create_game(payload, background_tasks)


@api_router.get("/games/{game_id}")
def get_game(game_id: str):
    try:
        return game_service.get_game(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/games/{game_id}/runtime-manifest")
def get_game_runtime_manifest(game_id: str):
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
    try:
        return game_service.create_session(game_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc


@api_router.get("/games/{game_id}/sessions/{session_id}")
def get_game_session(game_id: str, session_id: str):
    try:
        return game_service.get_session(game_id, session_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/games/{game_id}/sessions/{session_id}/choices")
def choose_game_session_edge(
    game_id: str, session_id: str, payload: GameChoiceRequest
):
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
    try:
        task = game_service.enqueue_node_video(game_id, node_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    background_tasks.add_task(
        game_service.run_node_video, task["id"], game_id, node_id
    )
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
    try:
        return game_service.update_node(game_id, node_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.post("/games/{game_id}/edges")
def create_game_edge(game_id: str, payload: GameEdgeCreate):
    try:
        return game_service.create_edge(game_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.put("/games/{game_id}/edges/{edge_id}")
def update_game_edge(game_id: str, edge_id: str, payload: GameEdgeUpdate):
    try:
        return game_service.update_edge(game_id, edge_id, payload)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.delete("/games/{game_id}/edges/{edge_id}", status_code=204)
def delete_game_edge(game_id: str, edge_id: str):
    try:
        game_service.delete_edge(game_id, edge_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


@api_router.get("/game-tasks/{task_id}")
def get_game_task(task_id: str):
    try:
        return game_service.get_task(task_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
