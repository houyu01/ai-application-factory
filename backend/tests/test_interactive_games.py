from src.application.game_service import InteractiveGameService
from src.domain.models import GameEdgeCreate, GameEdgeUpdate, GameNodeUpdate, InteractiveGameCreate
from src.infrastructure.interactive_game_repository import InteractiveGameRepository


def make_service(tmp_path) -> InteractiveGameService:
    return InteractiveGameService(InteractiveGameRepository(tmp_path / "game.db"))


def make_payload() -> InteractiveGameCreate:
    return InteractiveGameCreate(
        name="雾城抉择",
        script="主角在雾城醒来，必须在有限时间内作出选择，寻找失踪的同伴并承担后果。",
        success_ending_count=2,
        failure_ending_count=4,
        branch_min=2,
        branch_max=3,
    )


def test_game_creation_and_graph_decomposition(tmp_path):
    service = make_service(tmp_path)
    game = service.create_game(make_payload())

    assert game["status"] == "生成中"
    assert service.get_game(game["id"])["nodes"] == []

    service.decompose_game(game["task_id"], game["id"])
    saved = service.get_game(game["id"])

    assert saved["status"] == "生成成功"
    assert len(saved["assets"]) == 3
    assert len(saved["nodes"]) == 1 + 3 + 6 + 6
    assert len([node for node in saved["nodes"] if node["node_type"] == "success"]) == 2
    assert len([node for node in saved["nodes"] if node["node_type"] == "failure"]) == 4
    assert saved["edges"]
    assert saved["tasks"][0]["status"] == "生成成功"


def test_game_node_and_edge_can_be_edited(tmp_path):
    service = make_service(tmp_path)
    game = service.create_game(make_payload())
    service.decompose_game(game["task_id"], game["id"])
    saved = service.get_game(game["id"])
    first_node = saved["nodes"][0]
    second_node = saved["nodes"][1]

    updated_node = service.update_node(
        game["id"],
        first_node["id"],
        GameNodeUpdate(title="新的起点", duration_seconds=12),
    )
    assert updated_node["title"] == "新的起点"

    edge = service.create_edge(
        game["id"],
        GameEdgeCreate(
            source_node_id=first_node["id"],
            target_node_id=second_node["id"],
            option_text="观察雾中的灯光",
        ),
    )
    edge = service.update_edge(
        game["id"], edge["id"], GameEdgeUpdate(option_text="跟随雾中的灯光")
    )
    assert edge["option_text"] == "跟随雾中的灯光"
    service.delete_edge(game["id"], edge["id"])
    assert edge["id"] not in {item["id"] for item in service.get_game(game["id"])["edges"]}


def test_runtime_session_returns_video_choices_and_records_path(tmp_path):
    service = make_service(tmp_path)
    game = service.create_game(make_payload())
    service.decompose_game(game["task_id"], game["id"])
    saved = service.get_game(game["id"])
    session = service.create_session(game["id"])
    start_node = next(node for node in saved["nodes"] if node["node_type"] == "start")
    start_edge = next(
        edge for edge in saved["edges"] if edge["source_node_id"] == start_node["id"]
    )

    next_state = service.choose_session_edge(game["id"], session["id"], start_edge["id"])

    assert next_state["path"][0]["edge_id"] == start_edge["id"]
    assert next_state["current_node"]["id"] == start_edge["target_node_id"]
    assert next_state["choices"]
