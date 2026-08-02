"""Initial graph planner for interactive full-motion-video games.

This is a deterministic scaffold until the configured LLM operator is wired
in. It already emits a DAG-shaped graph, so the editor and runtime do not need
to change when the planner is replaced with an LLM implementation.
"""

from __future__ import annotations

from math import ceil
from typing import Any


class InteractiveGamePlanner:
    """Create a reviewable branching graph from game creation settings."""

    def plan(self, settings: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
        script = settings["script"]
        branch_min = int(settings["branch_min"])
        branch_max = int(settings["branch_max"])
        terminal_count = int(settings["success_ending_count"]) + int(
            settings["failure_ending_count"]
        )
        duration = (int(settings["node_duration_min"]) + int(settings["node_duration_max"])) // 2

        # Use the maximum number of choices for the first layer so the next
        # layer can be distributed evenly while staying inside the configured
        # branch range (the default becomes 4 -> 8 -> 32).
        first_count = branch_max
        middle_count = max(first_count * branch_min, ceil(terminal_count / branch_max))
        nodes: list[dict[str, Any]] = []
        edges: list[dict[str, Any]] = []

        def add_node(
            node_id: str,
            node_type: str,
            title: str,
            x: int,
            y: int,
            original_text: str,
        ) -> None:
            nodes.append(
                {
                    "id": node_id,
                    "node_type": node_type,
                    "title": title,
                    "original_text": original_text,
                    "prompt": (
                        f"游戏剧本：{script}\n"
                        f"节点类型：{node_type}\n"
                        f"节点目标：{title}\n"
                        "请生成可与前后视频连续衔接的镜头、表演、场景和转场提示词。"
                    ),
                    "duration_seconds": duration,
                    "status": "未生成",
                    "position_x": x,
                    "position_y": y,
                    "video_history": [],
                }
            )

        add_node("start", "start", "起始视频", 80, 260, script[:240])

        first_ids: list[str] = []
        for index in range(first_count):
            node_id = f"chapter_{index + 1}"
            first_ids.append(node_id)
            add_node(
                node_id,
                "normal",
                f"剧情节点 {index + 1}",
                360,
                80 + index * 150,
                script[:240],
            )
            edges.append(
                {
                    "id": f"start_option_{index + 1}",
                    "source_node_id": "start",
                    "target_node_id": node_id,
                    "option_text": f"选择路径 {index + 1}",
                    "sort_order": index + 1,
                }
            )

        middle_ids: list[str] = []
        for index in range(middle_count):
            node_id = f"branch_{index + 1}"
            middle_ids.append(node_id)
            add_node(
                node_id,
                "normal",
                f"分支过程 {index + 1}",
                650,
                50 + index * 120,
                script[:240],
            )
        for parent_index, parent in enumerate(first_ids):
            start = (parent_index * len(middle_ids)) // len(first_ids)
            end = ((parent_index + 1) * len(middle_ids)) // len(first_ids)
            children = middle_ids[start:end]
            if len(children) < branch_min:
                children.extend(
                    middle_ids[(start + offset) % len(middle_ids)]
                    for offset in range(branch_min - len(children))
                )
            for offset, target in enumerate(children[:branch_max], start=1):
                edges.append(
                    {
                        "id": f"{parent}_option_{offset}",
                        "source_node_id": parent,
                        "target_node_id": target,
                        "option_text": f"继续调查 {offset}",
                        "sort_order": offset,
                    }
                )

        terminal_ids: list[str] = []
        success_count = int(settings["success_ending_count"])
        for index in range(terminal_count):
            is_success = index < success_count
            node_id = f"ending_{index + 1}"
            terminal_ids.append(node_id)
            add_node(
                node_id,
                "success" if is_success else "failure",
                f"{'成功' if is_success else '失败'}结局 {index + 1 if is_success else index - success_count + 1}",
                980,
                40 + index * 90,
                script[-240:],
            )

        for index, source in enumerate(middle_ids):
            choice_count = min(branch_max, max(branch_min, ceil(terminal_count / middle_count)))
            for offset in range(choice_count):
                target = terminal_ids[(index * choice_count + offset) % len(terminal_ids)]
                edges.append(
                    {
                        "id": f"{source}_option_{offset + 1}",
                        "source_node_id": source,
                        "target_node_id": target,
                        "option_text": f"做出决定 {offset + 1}",
                        "sort_order": offset + 1,
                    }
                )

        assets = [
            {
                "id": "char_001",
                "type": "character",
                "name": "主角",
                "prompt": f"从互动游戏剧本中提取主角的身份、外貌、服装、性格和连续性特征：{script}",
                "status": "未生成",
            },
            {
                "id": "scene_001",
                "type": "scene",
                "name": "核心场景",
                "prompt": f"从互动游戏剧本中提取核心场景的空间、时间、光线和可连续复用的视觉特征：{script}",
                "status": "未生成",
            },
            {
                "id": "prop_001",
                "type": "prop",
                "name": "关键道具",
                "prompt": f"从互动游戏剧本中提取关键道具及其叙事作用、外观和连续性特征：{script}",
                "status": "未生成",
            },
        ]
        return {"assets": assets, "nodes": nodes, "edges": edges}
