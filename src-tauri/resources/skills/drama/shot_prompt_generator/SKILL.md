---
name: shot_prompt_generator
description: 将分镜文本和已保存的角色、场景、道具素材融合成带图片引用节点的视频生成富文本 Prompt。
metadata:
  agent: drama
  prompt_template_v1: 每个分镜 Prompt 生成 2～3 个连续镜头
  prompt_template_v2: 每个分镜 Prompt 只生成 1 个完整的连续长镜头，不要拆分镜头
---
输出富文本 Prompt 的 nodes JSON，不要输出 Markdown 或解释。nodes 只能由 text 和 reference 两种节点组成。text 节点保存可编辑文字；reference 节点必须包含已保存素材 catalog 中的 asset_id、asset_type 和 label，用于在编辑器中渲染成带缩略图的 @图N（素材名）引用胶囊。角色 catalog 若提供 variant_id，表示该角色的独立形态；当前分镜出现该形态时 reference 必须同时包含 asset_id、variant_id、asset_type、label，且 label 使用“角色名 · 形态名”，绝不可只选角色基础图。严格按“场景、角色、风格、光线、位置、镜头、配音”组织文字。场景、角色、道具和占位图段落先列出实际使用的 reference 节点；位置和每个镜头的动作中再次引用发生交互的素材。若分镜原文含“【人物首次出场：当前名字｜人物描述：…】”，必须保留该首次出场描述，并在人物第一次清晰入画的镜头写入“【人物姓名标识｜姓名：当前名字｜时长：1～2s｜位置：人物近旁且不遮挡脸部｜效果：快速淡入淡出】”；姓名以当前角色素材 label/name 为准，若原文名字已变化则优先使用当前素材名。该标识不是字幕，即使 subtitles 为 false 也必须保留，只能在该人物首次出场时出现一次。每个分镜 Prompt 生成 2～3 个连续镜头，镜头必须使用“【镜头N | 时长Xs | 时间：日 外】”开头，并紧跟一段“【配音：旁白｜VoiceID：...｜状态：...｜情绪：...｜语气特点：...｜台词：...】”。需要使用图片的地方必须输出 reference 节点，不要生硬拼接字段。必须优先使用已保存素材的描述，不要臆造不存在的人物、地点、道具或占位图；要交代镜头起始状态和结束状态，保证相邻分镜可衔接。图片引用必须使用 reference 节点，不能把 image_url 写进 text。如果角色 catalog 提供了 voice 和 voice_prompt，必须在配音段落中写出该角色的 VoiceID 音色名称，并补充状态、情绪、语气特点和台词；不要把音色描述误当成图片 reference。同步遵守给定的分辨率、字幕和背景音乐约束。当 subtitles 为 false 时，绝不输出字幕段落、字幕说明、字幕标记或“不要字幕”文字；人物姓名标识除外，配音段落仍需保留。当 background_music 为 false 时，绝不输出背景音乐、配乐、BGM 段落或“不要背景音乐”文字；配音、音效和环境音仍需保留。
