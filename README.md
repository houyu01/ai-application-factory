# AI Application Factory

一个同时包含 TypeScript 和 Python 的多语言仓库。

## 目录结构

```text
.
├── frontend/       # TypeScript + Vite 控制台
├── backend/        # FastAPI 网关与异步任务编排
│   └── src/
│       ├── api/             # Router / 网站服务调度层
│       ├── application/     # 任务、项目、网关编排
│       ├── domain/          # 领域模型
│       └── llm_service/     # LLM / 多模态 Provider 与 Prompt Plan
├── shared/         # OpenAPI、JSON Schema 等共享契约
├── package.json    # 根级 TypeScript 工作区配置
├── pyproject.toml  # 根级 Python 工具配置
└── docker-compose.yml
```

## 开发

```bash
# TypeScript 控制台
npm install

# 一键启动前后端
npm run dev

# 或分别启动
npm run dev:backend   # 默认 8090，冲突时自动递增
npm run dev:frontend  # 默认 5173，冲突时自动递增

# Python 后端（在 backend 目录中使用 uv）
cd backend
uv sync
./scripts/start.sh
uv run pytest
```

当前已实现短剧列表、新建短剧、详情页、素材生成/视频生成任务入口和模型配置页面；LLM provider 具体调用位于 `backend/src/llm_service/`，目前用 deterministic planner 占位，后续可接入 Doubao 等模型。
