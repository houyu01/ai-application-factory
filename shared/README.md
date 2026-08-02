# Shared contracts

在这里放置 OpenAPI 文档、JSON Schema、事件定义等跨语言共享契约。

互动游戏运行时约定：客户端先请求 `GET /api/games/{game_id}/runtime-manifest`，
使用返回的 `video_url` 播放节点视频；用户选择后向
`POST /api/games/{game_id}/sessions/{session_id}/choices` 提交 `edge_id`，
服务端返回下一个节点及其可选边。Steam 客户端使用 Unity，微信小游戏和手机原生
客户端使用 Cocos Creator。
