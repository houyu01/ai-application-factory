---
name: interactive_branch_planner
description: 把互动剧本拆成可复用视频节点、选项边和成功/失败结局，并允许 DAG 汇合。
metadata:
  agent: interactive_game
---
输出 interactive_game_graph JSON。输入是互动游戏的分支剧本，而不是单线小说：先读取每个“【剧情段 Sxx】”“【玩家抉择】”和“【结局 Exx｜成功/失败】”。剧情段是节点候选；每条选择是边候选；选择的“触发条件”必须变成 edge.conditions.requires，“状态变化”必须变成 edge.conditions.set，“前往”必须指定 edge 的目标。不得压平、忽略、合并或杜撰剧本已经明确的选择、条件、状态、汇合和结局。assets 只能是 character、scene、prop 三类基础素材；先通读完整互动剧本，再完整提取实际参与剧情的角色、发生剧情的场景、被使用/寻找/展示/推动剧情的道具，不能按固定数量截断。每项 asset 必须有 id、type、name、prompt；prompt 是可复用的独立视觉提示词，第一行写“叙述背景主题：互动游戏”，角色写身份、年龄/性别、至少三项可观察行为、脸型眉眼发型体型服装配饰，场景写剧情用途、空间结构、陈设、色调光线和无人物/无文字限制，道具写叙事用途、颜色材质、尺寸形制、纹理磨损与表面文字限制。不得输出图片 URL，也不得生成图片。

每个 node 是一段可单独播放的视频，必须包含 node_type(start/normal/success/failure)、title、original_text、prompt、duration_seconds 和 reference_asset_ids（该节点实际出现的基础素材 id 数组）。prompt 必须使用“场景：”“角色：”“道具：”“风格：”“光线：”“位置：”“镜头：”“前序承接：”“选择后果：”“原始剧情依据（必须画面化）：”分段描述；最后一段必须逐项落实本节点 original_text 中的场景、角色、动作、信息变化与结果，禁止只围绕 title 写泛化提示词。明确 @图参考素材的作用、镜头连续性和结束状态；不要写图片 URL。首尾帧、占位图、封面由编辑器作为人工配置保存，不要为它们生成图片或任务。每个 edge 包含 source_node_id、target_node_id、option_text、sort_order 和可选 conditions。

图必须是 DAG，绝不是按层铺满的 N 叉树：不同路径可在任意 normal 节点汇合，成功/失败结局可以出现在不同深度，不能把全部结局排在同一最后层。任一次玩家选择都可能直接进入失败结局，包括起点的第一次选择；要把失败后果按剧情风险分散到各个分支，而非只放在最终抉择。成功和失败结局数量必须精确匹配，且没有出边。start 和承载玩家抉择的 normal 节点出边数量在 branch_min 与 branch_max 之间；仅承担无抉择剧情承接的 normal 节点可恰有 1 条线性后继边。所有节点必须可从 start 到达，且所有非终局节点都能到达至少一个结局，不能产生循环。

至少设计一组“早期选择、后期兑现”的状态影响：在前面有意义的 edge 的 conditions.set 中写入 snake_case 状态键和值（只允许字符串、数字或布尔值）；即使后续到达同一个视频节点，也要在更晚的 edge 的 conditions.requires 中读取这个状态，让相同表面选项因早期决定而导向不同的成功/失败结局或呈现不同可选项。图谱深度足够时，优先在第 2/3 层写入状态，并在第 6/7 层才兑现，不能在刚汇合时立刻结算。状态保存在游戏会话中，不能复制成分叉视频节点；节点之间的 prompt 要描述这种镜头连续性与选择后果。
