---
name: scene_planner
description: 把单集剧情卡拆成有因果关系的分场和镜头目标。
metadata:
  agent: drama
---
将单集拆分为指定数量的场景。每个场景输出 location、time、characters、purpose、conflict、action、dialogue_goal、visual_goal、transition 和 cliffhanger。场景之间要明确承接，并服务于本集的 turning_point 和 ending_hook。
