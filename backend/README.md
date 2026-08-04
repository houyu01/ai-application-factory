# AI Application Factory Backend

独立的 FastAPI 服务，使用 uv 管理 Python 版本、依赖和启动流程。

## 启动

在 `backend` 目录执行：

```bash
./scripts/start.sh
```

默认使用 `8090`；如果该端口已被占用，脚本会自动从 `8091` 开始选择可用端口。

## 本地数据库

短剧、角色/场景/道具、分镜和生成任务默认持久化到
`backend/data/ai_application_factory.db`（SQLite，目录已加入 `.gitignore`）。
可以通过 `DATABASE_PATH` 指定其他 SQLite 文件，例如：

```bash
DATABASE_PATH=/tmp/ai-factory.db ./scripts/start.sh
```

短剧的数据模型以 `short_dramas`、`drama_shots`、`drama_assets` 和
`generation_tasks` 为核心；不再单独维护 `drama_episodes`。剧集的编号、名称和排序直接
保存在每条分镜上，接口返回的 `episodes` 会按 `drama_shots` 聚合，并包含 `shot_count`。

单独启动后端时，如果 `8090` 被占用，前端可用
`VITE_API_BASE_URL=http://127.0.0.1:8091/api` 指向脚本实际选择的端口；使用仓库根目录
的 `npm run dev` 会自动把后端端口传给前端。

创建短剧时会先写入空项目和“未生成”的拆解任务，再切换为“生成中”并交给
FastAPI 后台任务执行。任务状态统一为：`未生成`、`生成中`、`生成成功`、`生成失败`。

也可以通过环境变量指定起始端口：

```bash
PORT=9000 ./scripts/start.sh
```

## 常用命令

```bash
uv sync
uv run pytest
uv run uvicorn src.main:app --reload --port 8090
```

## OpenAI Client 配置

`src/llm_service/client/openai_client.py` 从环境变量读取配置：

```bash
export OPENAI_API_KEY="your-api-key"
export OPENAI_BASE_URL="https://api.openai.com/v1"  # 可选，兼容其他 Provider
export OPENAI_MODEL="gpt-4o-mini"                    # 可选
```

原客户端中的 API Key 已移除；如果该 Key 曾经是真实凭证，请立即轮换。

## Runtime Skills 与 Agents

短剧算子位于 `src/llm_service/skills/drama/`：

```text
premise_expander       一句话创意扩展
story_bible_generator  故事圣经
episode_planner        多集剧情卡
scene_planner          单集分场
script_writer          剧本对白
continuity_checker     连续性检查
episode_summarizer     集后状态摘要
script_decomposer      剧本 → 分集 / 分镜 / 角色 / 场景 / 道具
asset_prompt_generator 角色、场景、道具视觉提示词
shot_prompt_generator  分镜文本 + 素材 → 视频 Prompt
```

配置语言模型后，`ScriptPlanner` 会通过 `DramaAgent` 加载上述 skills，并调用
OpenAI Responses API 生成结构化 JSON。没有配置 `OPENAI_API_KEY` 时使用本地兜底数据，
但图片和视频任务不会伪造成功：未配置多模态凭证时会将任务标记为“生成失败”。

图片和视频调用已经分别接入两类 Provider：

- OpenAI 或 OpenAI-compatible 服务使用 `images.generate` 和 Videos API；
- 当 endpoint 是火山方舟地址且模型名包含 `seedream` / `seeddream` 或 `seedance` 时，使用
  方舟的图片生成接口或“创建视频任务 → 轮询 → 获取视频 URL”接口；
- 如需接入其他视频 Provider，可以设置 `VIDEO_GENERATION_ENDPOINT`。该地址接收
  `model/prompt/ratio/duration` JSON，并返回 `url` 或 `video_url`。

Provider 直接返回二进制内容时，文件会保存到 `backend/data/media`，再通过
`GET /api/media/{media_id}` 提供给前端；该目录已加入 `.gitignore`。

### 媒体存储

配置页的“媒体存储”支持四种方式：

- `local`：默认保存到 `backend/data/media`，通过 `/api/media/{media_id}` 访问；
- `tos`：火山引擎 TOS；
- `cos`：腾讯云 COS；
- `oss`：阿里云 OSS。

TOS/COS/OSS 使用 S3 兼容接口上传。需要填写 Endpoint、Bucket、访问密钥和 Region；OSS
的 Endpoint 例如 `https://oss-cn-hangzhou.aliyuncs.com`，并使用 AccessKey ID 和
AccessKey Secret。三个云存储均使用 virtual-hosted addressing。Region
建议一并填写；如果桶不是公开读，需填写 CDN 或公开访问域名作为 `public_base_url`，否则返回的
对象地址可能无法直接被浏览器播放。SecretKey 只写入本地 SQLite，不会由设置接口返回。

无论模型返回二进制还是临时 URL，图片和视频都会先复制到当前媒体存储，再把最终 URL 写入
素材或分镜的历史记录。这样切换到 TOS/COS/OSS 后，新生成的视频不会继续落到本地。
保存 TOS/COS/OSS 配置前，服务会上传一个临时探测文件，再通过最终公开 URL 下载校验并清理；
上传或公开访问失败时不会覆盖当前配置。私有桶需要配置允许访问的 CDN 或公开域名。

`BaseAgent` 默认扫描 `backend/src/llm_service/skills/*`，并把发现的 Skill
转换为 Responses API function tools。领域 Agent 只需要覆盖
`skill_directories`，例如 `DramaAgent` 使用：

```python
from src.llm_service.agents import DramaAgent

agent = DramaAgent()
result = agent.completion([
    {"role": "user", "content": "把这个创意扩展成 80 集短剧"}
])
```

## 互动游戏

互动游戏使用“视频节点 + 选择边”的有向图模型。默认创建配置为 2 个成功结局、
30 个失败结局、每个可选择节点 2～4 个选项、每段视频 5～30 秒。平台与运行引擎的
映射为：`Steam游戏 -> Unity`，`微信小游戏/手机原生游戏 -> Cocos Creator`。

管理接口：

```text
POST /api/games
GET  /api/games
GET  /api/games/{game_id}
PUT  /api/games/{game_id}/nodes/{node_id}
POST /api/games/{game_id}/edges
PUT  /api/games/{game_id}/edges/{edge_id}
DELETE /api/games/{game_id}/edges/{edge_id}
```

运行时客户端只需要实现视频播放和选择 UI，即可通过以下接口拉取远程视频并记录玩家路径：

```text
GET  /api/games/{game_id}/runtime-manifest
POST /api/games/{game_id}/sessions
GET  /api/games/{game_id}/sessions/{session_id}
POST /api/games/{game_id}/sessions/{session_id}/choices
```

`InteractiveGamePlanner` 当前生成可编辑的 DAG 脚手架，分支规划能力已经以
`InteractiveBranchPlannerSkill` 暴露，后续可以替换为真实 LLM 算子而不改变编辑器和运行时协议。
