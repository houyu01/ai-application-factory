from typing import Any

from ..domain.models import GenerationStatus
from ..llm_service.planner import ScriptPlanner


class TaskServiceAssetMixin:
    """Generate reusable drama assets requested by the editor.

    Asset panels and the placeholder-layout dialog call this workflow. It owns
    durable image tasks and converts editor-only layout guidance into clean
    provider-generated images suitable for downstream video references.
    """

    @staticmethod
    def _placeholder_prompt(
        project: dict[str, Any],
        scene: dict[str, Any],
        characters: list[dict[str, Any]],
        props: list[dict[str, Any]],
        placements: list[dict[str, Any]],
    ) -> str:
        position_lines = []
        for index, placement in enumerate(placements):
            role = next(
                (item for item in characters if item.get("id") == placement["asset_id"]),
                {},
            )
            horizontal = "左侧" if placement["x"] < 0.34 else "右侧" if placement["x"] > 0.66 else "中央"
            vertical = "前景" if placement["y"] > 0.5 else "中景" if placement["y"] > 0.22 else "后景"
            position_lines.append(
                f"参考图{index + 2}中的角色“{role.get('name', '角色')}”位于画面{horizontal}{vertical}，"
                f"相对位置 x={placement['x']:.2f}, y={placement['y']:.2f}，"
                f"画面占比宽={placement['width']:.2f}, 高={placement['height']:.2f}；"
                f"动作/备注：{placement.get('note') or '站立'}"
            )
        prop_lines = [
            f"道具“{item.get('name')}”：{item.get('prompt', '')}" for item in props
        ]
        return "\n".join(
            [
                "生成一张干净、完整、可直接提供给视频生成模型的镜头构图参考图。",
                f"场景：{scene.get('name', '未命名场景')}；场景提示词：{scene.get('prompt', '')}",
                f"风格：{project.get('style', '真人风格')}；背景主题：{project.get('theme', '')}；画幅：{project.get('ratio', '9:16')}",
                "参考图1是场景；后续参考图是角色和剧情相关道具。保持参考人物脸部、服装、道具材质与场景结构一致。",
                *position_lines,
                *prop_lines,
                "将角色和道具自然融合进场景并符合透视、遮挡、光影和比例关系。",
                "输出必须是无编辑痕迹的成片参考图：不要方框、字母、标签、箭头、辅助线、坐标、界面、文字或水印。",
            ]
        )

    @staticmethod
    def _normalize_placeholder_placements(
        placements: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        normalized: list[dict[str, Any]] = []
        for raw in placements:
            asset_id = str(raw.get("asset_id") or "").strip()
            if not asset_id:
                continue
            try:
                width = min(1.0, max(0.04, float(raw.get("width", 0.2))))
                height = min(1.0, max(0.04, float(raw.get("height", 0.35))))
                x = min(1.0 - width, max(0.0, float(raw.get("x", 0.28))))
                y = min(1.0 - height, max(0.0, float(raw.get("y", 0.26))))
            except (TypeError, ValueError):
                width, height, x, y = 0.2, 0.35, 0.28, 0.26
            normalized.append(
                {
                    "id": str(raw.get("id") or f"placement_{len(normalized) + 1}"),
                    "asset_id": asset_id,
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                    "pose": str(raw.get("pose") or "").strip(),
                    "note": str(raw.get("note") or raw.get("pose") or "").strip(),
                }
            )
        return normalized[:30]

    def _placeholder_reference_assets(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        scene: dict[str, Any],
        placements: list[dict[str, Any]],
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
        assets = {str(item.get("id")): item for item in project.get("assets", [])}
        characters: list[dict[str, Any]] = []
        for placement in placements:
            role = assets.get(str(placement.get("asset_id") or ""))
            if role is None or role.get("type") != "character":
                raise ValueError("占位图只能放置角色素材")
            if not role.get("image_url"):
                raise ValueError(f"角色“{role.get('name', '未命名')}”尚未生成图片")
            if role not in characters:
                characters.append(role)
        explicit_ids = {str(value) for value in shot.get("reference_asset_ids") or []}
        for node in shot.get("prompt_rich") or []:
            if isinstance(node, dict) and node.get("type") == "reference":
                explicit_ids.add(str(node.get("asset_id") or ""))
        context = "\n".join(
            str(shot.get(key) or "") for key in ("title", "original_text", "prompt")
        )
        props: list[dict[str, Any]] = []
        for candidate in assets.values():
            if candidate.get("type") != "prop":
                continue
            explicit = str(candidate.get("id")) in explicit_ids
            mentioned = bool(candidate.get("name") and str(candidate["name"]) in context)
            if not explicit and not mentioned:
                continue
            if not candidate.get("image_url"):
                if explicit:
                    raise ValueError(f"道具“{candidate.get('name', '未命名')}”尚未生成图片")
                continue
            props.append(candidate)
        return [scene, *characters, *props], characters, props

    def _generate_placeholder_image_url(
        self,
        project: dict[str, Any],
        placeholder: dict[str, Any],
        references: list[dict[str, Any]],
    ) -> str:
        options = self._provider_options(project, "multimodal")
        if not options.get("api_key"):
            raise RuntimeError("未配置图像模型 API Key，无法生成占位图")
        ratio = str(project.get("ratio") or "9:16")
        prompt = str(placeholder.get("prompt") or "")
        reference_images = [
            self._cover_reference_input(str(item["image_url"])) for item in references
        ]
        result = self._generate_provider_image(
            options, prompt, ratio=ratio, reference_images=reference_images
        )
        return self._persist_provider_result(result, ".png", "占位图模型")

    def run_asset_variant_image(
        self, task_id: str, project_id: str, asset_id: str, variant_id: str
    ) -> None:
        try:
            if self._asset_image_task_cancelled(task_id):
                return
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            variant = next(
                (
                    item
                    for item in asset.get("variants", [])
                    if str(item.get("id")) == variant_id
                ),
                None,
            )
            if variant is None:
                raise KeyError(f"Asset variant not found: {variant_id}")
            variant_asset = {
                **asset,
                "prompt": "\n\n".join(
                    part
                    for part in (
                        str(asset.get("prompt") or "").strip(),
                        str(variant.get("prompt") or "").strip(),
                    )
                    if part
                ),
            }
            image_url = self._generate_image_url(project, variant_asset)
            if self._asset_image_task_cancelled(task_id):
                return
            updated = self.repository.update_asset_variant_status(
                project_id,
                asset_id,
                variant_id,
                GenerationStatus.SUCCEEDED,
                image_url=image_url,
            )
            updated_variant = next(
                item for item in updated.get("variants", []) if str(item.get("id")) == variant_id
            )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "asset_id": asset_id,
                    "variant_id": variant_id,
                    "image_url": image_url,
                    "prompt": self._asset_generation_prompt(project, variant_asset),
                    "variant": updated_variant,
                },
            )
        except Exception as exc:
            if self._asset_image_task_cancelled(task_id):
                return
            self.repository.update_asset_variant_status(
                project_id, asset_id, variant_id, GenerationStatus.FAILED
            )
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def enqueue_asset_variant_image(
        self, project_id: str, asset_id: str, variant_id: str
    ) -> dict[str, Any]:
        project = self.repository.get_drama(project_id)
        asset = self.repository.get_asset(project_id, asset_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        variant = next(
            (item for item in asset.get("variants", []) if str(item.get("id")) == variant_id),
            None,
        )
        if variant is None:
            raise KeyError(f"Asset variant not found: {variant_id}")
        active_task = self.repository.get_active_task(
            project_id, "asset_variant_image", variant_id
        )
        if active_task is not None:
            return {**active_task, "_reused": True}
        task = self.repository.create_task(
            project_id,
            "asset_variant_image",
            variant_id,
            input_snapshot={
                "project_id": project_id,
                "asset_id": asset_id,
                "variant_id": variant_id,
                "type": "asset_variant_image",
            },
        )
        self.repository.update_asset_variant_status(
            project_id, asset_id, variant_id, GenerationStatus.GENERATING
        )
        return {
            **self.repository.update_task_status(task["id"], GenerationStatus.GENERATING),
            "_reused": False,
        }

    def enqueue_placeholder_image(
        self,
        project_id: str,
        shot_id: str,
        scene_asset_id: str,
        placements: list[dict[str, Any]],
    ) -> dict[str, Any]:
        project = self.get_project(project_id)
        shot = self.repository.get_shot(project_id, shot_id)
        scene = self.repository.get_asset(project_id, scene_asset_id)
        if shot is None:
            raise KeyError(f"Shot not found: {shot_id}")
        if scene is None:
            raise KeyError(f"Scene asset not found: {scene_asset_id}")
        if scene.get("type") != "scene":
            raise ValueError("占位图必须使用场景素材作为背景")
        if not scene.get("image_url"):
            raise ValueError("请先生成场景图片，再创建占位图")
        normalized_placements = self._normalize_placeholder_placements(placements)
        if not normalized_placements:
            raise ValueError("请至少添加一个角色到占位图")
        references, characters, props = self._placeholder_reference_assets(
            project, shot, scene, normalized_placements
        )

        active_task = self.repository.get_active_task_by_snapshot(
            project_id, "placeholder_image", "shot_id", shot_id
        )
        if active_task is not None:
            return {**active_task, "_reused": True}

        version = 1 + sum(
            1
            for asset in project.get("assets", [])
            if asset.get("type") == "placeholder"
            and (asset.get("metadata") or {}).get("shot_id") == shot_id
            and (asset.get("metadata") or {}).get("render_mode")
            == "generated_composite"
        )
        metadata = {
            "shot_id": shot_id,
            "scene_asset_id": scene_asset_id,
            "scene_name": scene.get("name", "场景"),
            "placements": normalized_placements,
            "version": version,
            "render_mode": "generated_composite",
            "character_asset_ids": [item["id"] for item in characters],
            "prop_asset_ids": [item["id"] for item in props],
            "reference_asset_ids": [item["id"] for item in references],
        }
        prompt = self._placeholder_prompt(
            project, scene, characters, props, normalized_placements
        )
        asset = self.repository.create_asset(
            project_id,
            "placeholder",
            f"{shot.get('title', '分镜')} · 占位图 {version}",
            prompt,
            metadata,
        )
        self.repository.update_asset_status(
            asset["id"], GenerationStatus.GENERATING
        )
        task = self.repository.create_task(
            project_id,
            "placeholder_image",
            asset["id"],
            input_snapshot={
                "project_id": project_id,
                "shot_id": shot_id,
                "asset_id": asset["id"],
                "scene_asset_id": scene_asset_id,
                "placements": normalized_placements,
                "reference_asset_ids": metadata["reference_asset_ids"],
                "render_mode": "generated_composite",
                "type": "placeholder_image",
            },
        )
        return {
            **self.repository.update_task_status(
                task["id"], GenerationStatus.GENERATING
            ),
            "_reused": False,
        }

    def run_placeholder_image(
        self, task_id: str, project_id: str, asset_id: str
    ) -> None:
        try:
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Placeholder asset not found: {asset_id}")
            metadata = asset.get("metadata") or {}
            reference_ids = [
                str(value) for value in metadata.get("reference_asset_ids") or []
            ]
            references = [
                item
                for item in project.get("assets", [])
                if str(item.get("id")) in reference_ids
            ]
            if len(references) != len(set(reference_ids)) or any(
                not item.get("image_url") for item in references
            ):
                raise ValueError("占位图引用的场景、角色或道具图片不可用")
            references.sort(key=lambda item: reference_ids.index(str(item.get("id"))))
            image_url = self._generate_placeholder_image_url(
                project, asset, references
            )
            self.repository.update_asset_status(
                asset_id, GenerationStatus.SUCCEEDED, image_url=image_url
            )
            shot_id = str(metadata.get("shot_id") or "")
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is not None:
                prompt_rich = list(shot.get("prompt_rich") or [])
                if not prompt_rich and shot.get("prompt"):
                    prompt_rich = [{"type": "text", "text": shot["prompt"]}]
                prompt_rich = [
                    node
                    for node in prompt_rich
                    if not (
                        isinstance(node, dict)
                        and (
                            (
                                node.get("type") == "reference"
                                and node.get("asset_type") == "placeholder"
                            )
                            or (
                                node.get("type") == "text"
                                and str(node.get("text") or "").strip()
                                in {"布局参考：", "占位图参考："}
                            )
                        )
                    )
                ]
                prompt_rich.extend(
                    [
                        {"type": "text", "text": "\n占位图参考："},
                        {
                            "type": "reference",
                            "asset_id": asset_id,
                            "asset_type": "placeholder",
                            "label": asset.get("name", "占位图"),
                            "image_url": image_url,
                        },
                    ]
                )
                self.repository.update_shot(
                    project_id,
                    shot_id,
                    prompt=ScriptPlanner.rich_prompt_to_text(prompt_rich),
                    prompt_rich=prompt_rich,
                    placeholder_scene_asset_id=str(metadata.get("scene_asset_id") or ""),
                    placeholder_placements=metadata.get("placements") or [],
                    status=GenerationStatus.NOT_GENERATED,
                )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "asset_id": asset_id,
                    "image_url": image_url,
                    "scene_asset_id": metadata.get("scene_asset_id"),
                    "placements": metadata.get("placements") or [],
                    "reference_asset_ids": reference_ids,
                    "render_mode": "generated_composite",
                },
            )
        except Exception as exc:
            self.repository.update_asset_status(asset_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
