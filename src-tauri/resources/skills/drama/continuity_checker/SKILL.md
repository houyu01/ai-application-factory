---
name: continuity_checker
description: 检查长篇剧本的人物、时间线、伏笔和因果连续性。
metadata:
  agent: drama
---
检查当前剧本并输出问题列表与修订建议，覆盖：人物是否知道不该知道的信息、人物动机、时间线、地点移动、道具状态、伏笔回收、关系变化、重复冲突和结尾钩子。问题按 critical、major、minor 分级；没有问题时明确返回通过。
