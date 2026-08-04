"""ORM models for interactive video games and runtime sessions."""

from __future__ import annotations

from sqlalchemy import Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from .base import ORMBase


class InteractiveGame(ORMBase):
    """A branching interactive-video project and its platform settings."""

    __tablename__ = "interactive_games"
    __table_args__ = {"comment": "Interactive video game projects and graph snapshots."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable game id; used by editor/runtime APIs and never changed.")
    name: Mapped[str] = mapped_column(String(200), comment="Game display name; changed when the user edits the game name.")
    script: Mapped[str] = mapped_column(Text, comment="Original interactive script; read by branch planning and never changed by graph edits.")
    platform: Mapped[str] = mapped_column(String(80), comment="Target platform; read by engine selection and changed in project settings.")
    style: Mapped[str] = mapped_column(String(80), comment="Visual style; read by graph/media generation and changed in project settings.")
    success_ending_count: Mapped[int] = mapped_column(Integer, comment="Requested success ending count; read by branch planning and changed in game settings.")
    failure_ending_count: Mapped[int] = mapped_column(Integer, comment="Requested failure ending count; read by branch planning and changed in game settings.")
    branch_min: Mapped[int] = mapped_column(Integer, comment="Minimum choices per branch; read by planning and changed in game settings.")
    branch_max: Mapped[int] = mapped_column(Integer, comment="Maximum choices per branch; read by planning and changed in game settings.")
    node_duration_min: Mapped[int] = mapped_column(Integer, comment="Minimum node video duration; read by planning and changed in game settings.")
    node_duration_max: Mapped[int] = mapped_column(Integer, comment="Maximum node video duration; read by planning and changed in game settings.")
    language_model: Mapped[str] = mapped_column(String(200), comment="Selected language model; read by branch planning and changed in game settings.")
    multimodal_model: Mapped[str] = mapped_column(String(200), comment="Selected image model; read by asset generation and changed in game settings.")
    video_model: Mapped[str] = mapped_column(String(200), default="doubao-seedance-2.0", server_default="doubao-seedance-2.0", comment="Selected video model; read by node video generation and changed in game settings.")
    status: Mapped[str] = mapped_column(String(40), comment="Game generation status; changed by durable planning tasks.")
    assets_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Graph asset snapshot; read for compatibility and updated when graph planning is saved.")
    nodes_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Node graph snapshot; read for compatibility and updated when graph planning is saved.")
    edges_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Choice edge snapshot; read for compatibility and updated when graph planning is saved.")
    historical_videos_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Game-level video history snapshot; read by list/detail APIs and appended after successful node video generation.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Game creation timestamp; read for list ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last game update timestamp; changed by graph and setting edits.")


class GameAsset(ORMBase):
    """A reusable character, scene, or prop asset in an interactive game."""

    __tablename__ = "game_assets"
    __table_args__ = {"comment": "Interactive-game visual asset records."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable asset id; referenced by game prompts and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; scopes asset queries and never changed.")
    type: Mapped[str] = mapped_column(String(40), comment="Asset kind; read by editor and provider adapters.")
    name: Mapped[str] = mapped_column(String(200), comment="Asset display name; changed by manual asset editing.")
    prompt: Mapped[str] = mapped_column(Text, comment="Asset prompt; read by image generation and changed by editing.")
    image_url: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Current image URL; read by editor/runtime preparation and changed after upload/generation.")
    status: Mapped[str] = mapped_column(String(40), comment="Asset status; changed by image tasks.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Asset creation timestamp; read for ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last asset update timestamp; changed by asset edits and image tasks.")


class GameNode(ORMBase):
    """A playable video node in the interactive game's directed graph."""

    __tablename__ = "game_nodes"
    __table_args__ = {"comment": "Interactive game video nodes, endings, and playback history."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable node id; referenced by choice edges and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; scopes graph queries and never changed.")
    node_type: Mapped[str] = mapped_column(String(40), comment="start, normal, success, or failure node type; read by runtime.")
    title: Mapped[str] = mapped_column(String(200), comment="Node title; displayed in editor/runtime and changed by editing.")
    original_text: Mapped[str] = mapped_column(Text, comment="Node source story text; read by prompt generation and changed by editing.")
    prompt: Mapped[str] = mapped_column(Text, comment="Node video prompt; read by video generation and changed by prompt editing.")
    video_url: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Current playable video URL; set after generation and read by runtime.")
    duration_seconds: Mapped[int] = mapped_column(Integer, comment="Node video duration; read by editor/runtime and changed by editing.")
    status: Mapped[str] = mapped_column(String(40), comment="Node generation status; changed by video tasks.")
    position_x: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Canvas X coordinate; read by editor and changed by graph layout.")
    position_y: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Canvas Y coordinate; read by editor and changed by graph layout.")
    video_history_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Historical node video URLs; read by editor and appended after successful generation.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Node creation timestamp; read for ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last node update timestamp; changed by edits and video tasks.")


class GameEdge(ORMBase):
    """A selectable transition between two interactive-game video nodes."""

    __tablename__ = "game_edges"
    __table_args__ = {"comment": "Interactive game choices connecting source and target nodes."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable choice edge id; referenced by sessions and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; scopes edge queries and never changed.")
    source_node_id: Mapped[str] = mapped_column(String(100), comment="Node where the choice appears; changed only when the edge is recreated.")
    target_node_id: Mapped[str] = mapped_column(String(100), comment="Node reached after selection; changed by edge editing.")
    option_text: Mapped[str] = mapped_column(String(200), comment="Choice text shown to the player; changed by edge editing.")
    sort_order: Mapped[int] = mapped_column(Integer, default=1, server_default="1", comment="Choice ordering; changed by edge editing.")
    conditions_json: Mapped[str] = mapped_column(Text, default="{}", server_default="{}", comment="Optional runtime conditions; read by runtime and changed by graph editing.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Edge creation timestamp; read for ordering and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last edge update timestamp; changed by edge editing.")


class GameTask(ORMBase):
    """Durable interactive-game planning or node-video task."""

    __tablename__ = "game_tasks"
    __table_args__ = {"comment": "Interactive game generation tasks that survive refresh and restart."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable task id; polled by editor and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; scopes task queries and never changed.")
    type: Mapped[str] = mapped_column(String(80), comment="Task operation type; read by worker handlers and never changed.")
    resource_id: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Affected node or asset id; read by status UI.")
    status: Mapped[str] = mapped_column(String(40), comment="Durable task status; changed by worker transitions.")
    input_snapshot_json: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Restart recovery input; written at enqueue and read by workers.")
    result_json: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Completed task result; written on completion and returned by task APIs.")
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True, comment="Failure detail; written on failure and shown in the editor.")
    progress: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Task progress percentage; changed by worker and read by loading states.")
    stage: Mapped[str] = mapped_column(String(120), default="", server_default="", comment="Human-readable worker stage; changed during long-running work.")
    poll_attempts: Mapped[int] = mapped_column(Integer, default=0, server_default="0", comment="Provider polling attempts; incremented by the worker.")
    poll_lease_token: Mapped[str | None] = mapped_column(String(100), nullable=True, comment="Worker lease token; changed when a task is claimed.")
    poll_lease_until: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Worker lease expiry; read during claim and changed on lease refresh.")
    next_poll_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Next poll time; changed after provider polling.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Task creation timestamp; read for ordering and never changed.")
    started_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="First execution timestamp; set when work starts.")
    completed_at: Mapped[str | None] = mapped_column(String(40), nullable=True, comment="Completion timestamp; set on success or failure.")


class GameSession(ORMBase):
    """A player's current position and path through a game graph."""

    __tablename__ = "game_sessions"
    __table_args__ = {"comment": "Interactive game runtime sessions and current node state."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable session id; returned to the game client and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; scopes runtime reads and never changed.")
    current_node_id: Mapped[str] = mapped_column(String(100), comment="Current playable node; changed after a choice is selected.")
    status: Mapped[str] = mapped_column(String(40), comment="active or completed session state; changed when an ending is reached.")
    path_json: Mapped[str] = mapped_column(Text, default="[]", server_default="[]", comment="Selected edge path; appended after each choice and read for analytics/runtime.")
    created_at: Mapped[str] = mapped_column(String(40), comment="Session creation timestamp; read for analytics and never changed.")
    updated_at: Mapped[str] = mapped_column(String(40), comment="Last choice timestamp; changed after each selection.")


class GameChoiceEvent(ORMBase):
    """An append-only runtime event recording one player choice."""

    __tablename__ = "game_choice_events"
    __table_args__ = {"comment": "Append-only interactive game choice events for path tracking."}

    id: Mapped[str] = mapped_column(String(100), primary_key=True, comment="Stable choice event id; never changed.")
    session_id: Mapped[str] = mapped_column(String(100), index=True, comment="Session that made the choice; read for path history and never changed.")
    game_id: Mapped[str] = mapped_column(String(100), index=True, comment="Owning game id; read for analytics and never changed.")
    source_node_id: Mapped[str] = mapped_column(String(100), comment="Node where the choice was made; read for analytics and never changed.")
    edge_id: Mapped[str] = mapped_column(String(100), comment="Selected choice edge; read for analytics and never changed.")
    target_node_id: Mapped[str] = mapped_column(String(100), comment="Node reached by the choice; read for analytics and never changed.")
    option_text: Mapped[str] = mapped_column(String(200), comment="Choice text snapshot; read for analytics and never changed.")
    selected_at: Mapped[str] = mapped_column(String(40), comment="Choice timestamp; read for analytics and never changed.")
