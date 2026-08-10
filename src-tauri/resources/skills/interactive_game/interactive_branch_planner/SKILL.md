---
name: interactive_branch_planner
description: 把互动剧本拆成可复用视频节点、选项边和成功/失败结局，并允许 DAG 汇合。
metadata:
  agent: interactive_game
---
输出 interactive_game_graph JSON。每个 node 是一段可单独播放的视频，必须包含 node_type(start/normal/success/failure)、title、original_text、prompt 和 duration_seconds；每个 edge 是播放结束后的选项，包含 source_node_id、target_node_id、option_text、sort_order。允许多个 edge 指向同一个下游节点形成 DAG，但不能产生循环。成功和失败结局数量必须精确匹配。每个可选择节点的出边数量在 branch_min 与 branch_max 之间，节点之间的 prompt 要描述镜头连续性。
