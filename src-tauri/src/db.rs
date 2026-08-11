//! SQLite compatibility boundary for the local-first desktop application.

use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags};

use crate::{error::AppResult, migration::migrate_legacy_schema, system_voice_samples, value::now};

const CHARACTER_INTRODUCTION_TEMPLATE_RULE: &str = "若分镜原文包含“【人物首次出场：当前名字｜人物描述：…】”，保留该描述；在人物第一次清晰入画的对应镜头加入“【人物姓名标识｜姓名：当前角色素材 name｜时长：1～2s｜位置：人物近旁且不遮挡脸部｜效果：快速淡入淡出】”。姓名必须使用当前角色素材的 name；它不是字幕，即使 subtitles 为 false 也必须保留，并且同一人物只能展示一次。";

/// Owns short-lived SQLite connections, schema upgrades, and bundled seed data.
#[derive(Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    /// Open or create the app-owned SQLite database under the user's application-data directory.
    pub fn open(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let database = Self { path };
        database.with_connection(|connection| {
            connection.execute_batch(SCHEMA)?;
            migrate_legacy_schema(connection)?;
            seed_prompt_templates(connection)?;
            seed_voice_presets(connection)?;
            Ok(())
        })?;
        Ok(database)
    }

    /// Return the local database path so backup, media, and compatibility flows share one root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run a small transactional persistence action on a fresh connection.
    pub fn with_connection<T>(
        &self,
        action: impl FnOnce(&mut Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        connection.busy_timeout(std::time::Duration::from_secs(15))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        action(&mut connection)
    }
}

fn seed_prompt_templates(connection: &Connection) -> AppResult<()> {
    for source in [
        include_str!("../resources/prompt_templates/drama/shot_prompt_v1.json"),
        include_str!("../resources/prompt_templates/drama/shot_prompt_v2.json"),
        include_str!("../resources/prompt_templates/drama/shot_quality_v1.json"),
    ] {
        let payload: serde_json::Value = serde_json::from_str(source)?;
        let scope = payload["scope"].as_str().unwrap_or("drama");
        let name = payload["name"].as_str().unwrap_or("template");
        let version = payload["version"].as_str().unwrap_or("v1");
        let template = payload["template"].as_str().unwrap_or_default();
        let timestamp = now();
        connection.execute(
            "INSERT OR IGNORE INTO prompt_templates (id, scope, name, version, template_text, metadata_json, active, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)",
            rusqlite::params![format!("{scope}:{name}:{version}"), scope, name, version, template, payload["metadata"].to_string(), timestamp],
        )?;
        let previous_bundled_template = template.replace(CHARACTER_INTRODUCTION_TEMPLATE_RULE, "");
        for legacy in [
            legacy_template(scope, name, version),
            previous_bundled_template.as_str(),
        ] {
            connection.execute(
                "UPDATE prompt_templates SET template_text=?1,metadata_json=?2,updated_at=?3 WHERE id=?4 AND template_text=?5",
                rusqlite::params![template, payload["metadata"].to_string(), timestamp, format!("{scope}:{name}:{version}"), legacy],
            )?;
        }
    }
    Ok(())
}

/// Upgrade bundled Tauri defaults without overwriting a creator's edited or newer template version.
fn legacy_template(scope: &str, name: &str, version: &str) -> &'static str {
    match (scope, name, version) {
        ("drama", "shot_prompt", "v1") => "根据已保存的素材目录和分镜原文，生成可编辑的富文本分镜提示词。严格输出 JSON：{\"nodes\":[{\"type\":\"text\",\"text\":\"文本\"},{\"type\":\"reference\",\"asset_id\":\"素材ID\",\"asset_type\":\"character|scene|prop|placeholder\",\"label\":\"素材名称\"}]}。文本顺序必须是：场景、角色、风格、光线、位置、2到3个连续镜头、每个镜头对应的配音。引用图片的位置必须使用 reference 节点，不能写图片 URL，不能虚构素材。每个镜头以【镜头N | 时长Xs | 时间：日 外】开头，并包含机位、动作、光线、连续性和配音信息。若 subtitles 为 false，省略所有字幕段落、字幕说明和字幕标记，但保留配音信息。若 background_music 为 false，省略所有背景音乐、配乐和 BGM 段落，但保留配音、音效和环境音。",
        ("drama", "shot_prompt", "v2") => "根据已保存的素材目录和分镜原文，生成可编辑的富文本分镜提示词。严格输出 JSON：{\"nodes\":[{\"type\":\"text\",\"text\":\"文本\"},{\"type\":\"reference\",\"asset_id\":\"素材ID\",\"asset_type\":\"character|scene|prop|placeholder\",\"label\":\"素材名称\"}]}。文本顺序必须是：场景、角色、风格、光线、位置、一个完整连续的长镜头、该镜头对应的配音。只允许一个以【镜头1 | 时长Xs | 时间：日 外】开头的镜头段落；镜头在完整时长内连续推进，不要切镜头或跳切。引用图片的位置必须使用 reference 节点，不能写图片 URL，不能虚构素材。镜头必须包含机位、动作、光线、起始状态、结束状态、人物和道具的空间关系以及配音信息。若 subtitles 为 false，省略所有字幕段落、字幕说明和字幕标记，但保留配音信息。若 background_music 为 false，省略所有背景音乐、配乐和 BGM 段落，但保留配音、音效和环境音。",
        ("drama", "shot_quality", "v1") => "检查分镜是否可以直接交给视频模型生成。检查：分镜文本是否只描述一个连续场景，角色/场景/道具引用是否存在且有图片，镜头时长是否在配置范围，动作是否连续，镜头语言是否完整，台词/配音是否明确，是否违反字幕、背景音乐、Logo约束，是否出现图片URL或技术标识。输出 JSON：{\"status\":\"通过|需修改\",\"score\":0-100,\"issues\":[{\"code\":\"问题码\",\"severity\":\"error|warning\",\"message\":\"问题\",\"field\":\"字段\"}],\"checks\":{}}。",
        _ => "",
    }
}

fn seed_voice_presets(connection: &Connection) -> AppResult<()> {
    for (index, (id, name, gender, prompt)) in VOICES.iter().enumerate() {
        let timestamp = now();
        connection.execute(
            "INSERT OR IGNORE INTO voice_presets (id, name, gender, prompt, sort_order, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            rusqlite::params![id, name, gender, prompt, index as i64, timestamp],
        )?;
        if let Some(audio_url) = system_voice_samples::audio_url(id) {
            connection.execute(
                "UPDATE voice_presets SET audio_url=?1,updated_at=?2 WHERE id=?3 AND (audio_url IS NULL OR audio_url<>?1)",
                rusqlite::params![audio_url, timestamp, id],
            )?;
            connection.execute(
                "DELETE FROM voice_generation_tasks WHERE voice_id=?1",
                rusqlite::params![id],
            )?;
        }
    }
    Ok(())
}

const VOICES: &[(&str, &str, &str, &str)] = &[
    ("none", "不设置", "", ""),
    ("broken_whisper_resilient_female", "破碎感低语坚韧音（女）", "女", "女声压低至耳语，气息微微发颤带着一触即碎的脆弱感，声线纤细单薄，看似摇摇欲坠，基底却绷着一股不肯妥协的韧劲，温柔易碎，又绝不示弱。"),
    ("cold_boss_male", "冷酷霸总音（男）", "男", "成年男性低沉有磁性的声线，语速从容，语气冷静克制，字句带有不容置疑的掌控感。"),
    ("cool_career_newcomer_male", "清冷职场新人音（男）", "男", "年轻男性清透偏冷的声线，吐字清晰，语气礼貌而有距离感。"),
    ("soft_puppy_boyfriend_male", "奶狗软萌男友音（男）", "男", "年轻男性明亮柔软的声线，带有自然亲近感和轻微撒娇感，语气真诚直接。"),
    ("sickly_gloomy_yandere_male", "病娇阴郁疯批音（男）", "男", "男性偏低的阴郁声线，气息收紧，语调平静得近乎异常。"),
    ("ruthless_old_fox_male", "狠戾流老狐狸音（男）", "男", "成熟男性沙哑低沉的声线，语气老练圆滑，谈笑间带着试探和锋利感。"),
    ("arrogant_genius_male", "傲慢天才狂气音（男）", "男", "年轻男性清亮且张扬的声线，语速利落，语气自信。"),
    ("cool_abstinent_detective_female", "清冷禁欲刑警音（女）", "女", "成年女性清冷干净的声线，吐字利落，语气克制专业。"),
    ("warm_older_brother_male", "温柔大哥哥音（男）", "男", "成年男性温暖沉稳的声线，音色宽厚，语气耐心可靠。"),
    ("sweet_cold_yandere_male", "甜冷病娇音（男）", "男", "男性清甜柔和的声线中带着冷感，平静说话时显得亲昵温柔。"),
    ("cold_royal_sister_female", "冷酷御姐音（女）", "女", "成年女性低沉有力量的声线，语速干练，语气冷静果断。"),
    ("strong_female_lead", "女强角色音（女）", "女", "女性明亮坚定的声线，吐字清晰有力度，语气果断而有行动感。"),
    ("mature_warm_goddess_female", "成熟温柔女神音（女）", "女", "成年女性柔和成熟的声线，音色细腻从容，语气温柔但不软弱。"),
    ("sweet_fox_tease_female", "绿茶甜心撒娇小狐狸音（女）", "女", "年轻女性甜软灵动的声线，语气亲昵，尾音带一点撒娇和若有若无的试探。"),
];

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS short_dramas (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, script TEXT NOT NULL, expanded_script TEXT NOT NULL DEFAULT '', ratio TEXT NOT NULL, style TEXT NOT NULL, theme TEXT NOT NULL, language_model TEXT NOT NULL, multimodal_model TEXT NOT NULL, video_model TEXT NOT NULL DEFAULT 'doubao-seedance-2.0', episode_count INTEGER NOT NULL DEFAULT 25, enable_web_search INTEGER NOT NULL DEFAULT 0, expanded_script_min_chars INTEGER NOT NULL DEFAULT 5000, expanded_script_max_chars INTEGER NOT NULL DEFAULT 10000, shot_script_max_chars INTEGER NOT NULL DEFAULT 400, resolution TEXT NOT NULL DEFAULT '720p', video_public_prompt TEXT NOT NULL DEFAULT '', asset_public_prompts_json TEXT NOT NULL DEFAULT '{}', shot_constraints_json TEXT NOT NULL DEFAULT '{}', status TEXT NOT NULL, shots_json TEXT NOT NULL DEFAULT '[]', assets_json TEXT NOT NULL DEFAULT '[]', historical_videos_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS drama_assets (
 id TEXT PRIMARY KEY, drama_id TEXT NOT NULL, type TEXT NOT NULL, name TEXT NOT NULL, prompt TEXT NOT NULL, voice_id TEXT, image_url TEXT, content_hash TEXT, source_type TEXT NOT NULL DEFAULT 'generated', image_history_json TEXT NOT NULL DEFAULT '[]', variants_json TEXT NOT NULL DEFAULT '[]', metadata_json TEXT NOT NULL DEFAULT '{}', status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS drama_shots (
 id TEXT PRIMARY KEY, drama_id TEXT NOT NULL, episode_id TEXT NOT NULL DEFAULT '', episode_name TEXT NOT NULL, episode_sort_order INTEGER NOT NULL DEFAULT 1, shot_index INTEGER NOT NULL, title TEXT NOT NULL, original_text TEXT NOT NULL, duration_seconds INTEGER NOT NULL DEFAULT 10, prompt TEXT NOT NULL DEFAULT '', prompt_rich_json TEXT NOT NULL DEFAULT '[]', placeholder_scene_asset_id TEXT, placeholder_placements_json TEXT NOT NULL DEFAULT '[]', structured_json TEXT NOT NULL DEFAULT '{}', quality_json TEXT NOT NULL DEFAULT '{}', quality_status TEXT NOT NULL DEFAULT '未检查', quality_issues_json TEXT NOT NULL DEFAULT '[]', reference_asset_ids_json TEXT NOT NULL DEFAULT '[]', prompt_template_id TEXT, prompt_template_version TEXT NOT NULL DEFAULT 'v1', status TEXT NOT NULL, historical_videos_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS generation_tasks (
 id TEXT PRIMARY KEY, drama_id TEXT NOT NULL, type TEXT NOT NULL, job_id TEXT NOT NULL DEFAULT '', task_no INTEGER NOT NULL DEFAULT 1, trigger_type TEXT NOT NULL DEFAULT 'GENERIC', resource_id TEXT, status TEXT NOT NULL, input_snapshot_json TEXT, output_result_json TEXT, result_json TEXT, error_message TEXT, duration_ms INTEGER, poll_attempts INTEGER NOT NULL DEFAULT 0, poll_lease_token TEXT, poll_lease_until TEXT, provider_task_id TEXT, progress INTEGER NOT NULL DEFAULT 0, stage TEXT NOT NULL DEFAULT '', next_poll_at TEXT, created_at TEXT NOT NULL, started_at TEXT, finished_at TEXT, completed_at TEXT, FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS drama_shot_versions (
 id TEXT PRIMARY KEY, drama_id TEXT NOT NULL, shot_id TEXT NOT NULL, task_id TEXT, version_no INTEGER NOT NULL, status TEXT NOT NULL,
 -- Prompt and rich references are frozen when the version is created so a later refinement uses the original generation inputs.
 prompt TEXT NOT NULL DEFAULT '', prompt_rich_json TEXT NOT NULL DEFAULT '[]', structured_json TEXT NOT NULL DEFAULT '{}', quality_json TEXT NOT NULL DEFAULT '{}',
 -- Refinement provenance identifies the selected history item, its source video, and the creator's saved feedback.
 refinement_source_version_id TEXT, source_video_url TEXT, refinement_prompt TEXT NOT NULL DEFAULT '',
 provider_task_id TEXT, progress INTEGER NOT NULL DEFAULT 0, video_url TEXT, error_message TEXT,
 -- The creator's one selected completed version per shot, used as the default in the ZIP export dialog.
 is_selected_for_export INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, completed_at TEXT,
 FOREIGN KEY (drama_id) REFERENCES short_dramas(id) ON DELETE CASCADE, FOREIGN KEY (shot_id) REFERENCES drama_shots(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS prompt_templates (id TEXT PRIMARY KEY, scope TEXT NOT NULL, name TEXT NOT NULL, version TEXT NOT NULL, template_text TEXT NOT NULL, metadata_json TEXT NOT NULL DEFAULT '{}', active INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(scope, name, version));
CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS voice_presets (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, gender TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL DEFAULT '', audio_url TEXT, sort_order INTEGER NOT NULL DEFAULT 0, enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS voice_generation_tasks (
 id TEXT PRIMARY KEY,
 -- Existing catalog voice updated by this task; NULL denotes an unconfirmed custom-voice preview.
 voice_id TEXT,
 -- Creator-entered title, gender, and description are frozen before audio generation starts.
 name TEXT NOT NULL, gender TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL,
 -- The sentence spoken by the configured audio model, derived from the frozen voice metadata.
 sample_text TEXT NOT NULL,
 -- Durable queue state, progress, result URL, and diagnostic exposed by the settings preview UI.
 status TEXT NOT NULL, progress INTEGER NOT NULL DEFAULT 0, stage TEXT NOT NULL DEFAULT '', audio_url TEXT, error_message TEXT,
 created_at TEXT NOT NULL, completed_at TEXT
);
CREATE TABLE IF NOT EXISTS interactive_games (
 id TEXT PRIMARY KEY,
 -- Creator-visible game title, used in the workbench and packaged runtime manifest.
 name TEXT NOT NULL,
 -- Original creator input retained as the source for an expansion retry.
 script TEXT NOT NULL,
 -- Expanded narrative used as the only source for video-node graph planning.
 expanded_script TEXT NOT NULL DEFAULT '',
 -- Target runtime selected before a graph is created.
 platform TEXT NOT NULL,
 -- Visual direction shared by every reusable asset and video node.
 style TEXT NOT NULL,
 -- Exact number of terminal nodes that represent player success.
 success_ending_count INTEGER NOT NULL,
 -- Exact number of terminal nodes that represent player failure.
 failure_ending_count INTEGER NOT NULL,
 -- Inclusive minimum number of choices displayed after a playable node.
 branch_min INTEGER NOT NULL,
 -- Inclusive maximum number of choices displayed after a playable node.
 branch_max INTEGER NOT NULL,
 -- Inclusive minimum duration requested for each generated node video.
 node_duration_min INTEGER NOT NULL,
 -- Inclusive maximum duration requested for each generated node video.
 node_duration_max INTEGER NOT NULL,
 -- Language model selected for expansion and graph planning.
 language_model TEXT NOT NULL,
 -- Image model retained for the next asset-generation stage.
 multimodal_model TEXT NOT NULL,
 -- Video model retained for node-video generation.
 video_model TEXT NOT NULL DEFAULT 'doubao-seedance-2.0',
 -- Per-material-type image-generation instructions shared by every material in the workbench.
 asset_public_prompts_json TEXT NOT NULL DEFAULT '{}',
 -- Resolution supplied to node prompts and the video provider.
 resolution TEXT NOT NULL DEFAULT '720p',
 -- Whether the expansion provider may use its built-in web-search capability.
 enable_web_search INTEGER NOT NULL DEFAULT 0,
 -- Lower bound for the expanded story text requested from the language model.
 expanded_script_min_chars INTEGER NOT NULL DEFAULT 5000,
 -- Upper bound for the expanded story text requested from the language model.
 expanded_script_max_chars INTEGER NOT NULL DEFAULT 10000,
 -- Maximum source-text length stored in any one playable video node.
 node_script_max_chars INTEGER NOT NULL DEFAULT 400,
 -- Aggregate generation state shown in the game list and workbench header.
 status TEXT NOT NULL,
 -- Compatibility snapshot of reusable assets kept alongside normalized rows.
 assets_json TEXT NOT NULL DEFAULT '[]',
 -- Compatibility snapshot of graph nodes kept alongside normalized rows.
 nodes_json TEXT NOT NULL DEFAULT '[]',
 -- Compatibility snapshot of graph choice edges kept alongside normalized rows.
 edges_json TEXT NOT NULL DEFAULT '[]',
 -- Legacy aggregate history reserved for runtime and packaging compatibility.
 historical_videos_json TEXT NOT NULL DEFAULT '[]',
 -- Creation timestamp used for list ordering.
 created_at TEXT NOT NULL,
 -- Timestamp updated by any game-level configuration or graph change.
 updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS game_assets (
 id TEXT PRIMARY KEY, game_id TEXT NOT NULL,
 -- character, scene, prop, placeholder, or cover; determines its workbench grouping and allowable video references.
 type TEXT NOT NULL,
 -- Creator-visible material name, retained across graph edits and used in video-reference selectors.
 name TEXT NOT NULL,
 -- Editable visual prompt used as the source text for each independent image-generation task.
 prompt TEXT NOT NULL,
 -- Optional catalog voice selected for a character; passed as an audio reference by supporting video models.
 voice_id TEXT,
 -- Latest creator-uploaded or generated image used as a video reference or a first/last frame.
 image_url TEXT,
 -- Earlier image results retained for comparison and restoration in the material workbench.
 image_history_json TEXT NOT NULL DEFAULT '[]',
 -- Alternate poses, outfits, or states, each with their own image history and generation state.
 variants_json TEXT NOT NULL DEFAULT '[]',
 -- Cover or placeholder output settings and selected reference material IDs for durable image tasks.
 metadata_json TEXT NOT NULL DEFAULT '{}',
 -- Material generation state displayed by the workbench card.
 status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS game_nodes (
 id TEXT PRIMARY KEY, game_id TEXT NOT NULL, node_type TEXT NOT NULL, title TEXT NOT NULL, original_text TEXT NOT NULL,
	-- Editable video-generation prompt saved independently from the original story text.
	prompt TEXT NOT NULL,
	-- Rich prompt nodes retain each creator-placed @ reference chip across editor refreshes.
	prompt_rich_json TEXT NOT NULL DEFAULT '[]',
	-- Selected multi-shot or long-shot template used when this node regenerates its rich video prompt.
	prompt_template_version TEXT NOT NULL DEFAULT 'v1',
	video_url TEXT,
	-- Completed history entry explicitly selected for this node; the editor preview and game runtime keep using it until the creator changes it.
	selected_video_id TEXT,
	duration_seconds INTEGER NOT NULL, status TEXT NOT NULL, position_x INTEGER NOT NULL DEFAULT 0, position_y INTEGER NOT NULL DEFAULT 0,
 -- Selected reusable game-material IDs supplied to the video provider as reference images when their URLs are configured.
 reference_asset_ids_json TEXT NOT NULL DEFAULT '[]',
 -- Optional first and last frame material selections, stored as {"first":{"asset_id":...},"last":{"asset_id":...}}.
 first_last_frames_json TEXT NOT NULL DEFAULT '{}',
 -- Generated placeholder material used to establish this node's composition before video generation.
 placeholder_asset_id TEXT,
 -- Scene background selected while editing the node's placeholder composition.
 placeholder_scene_asset_id TEXT,
 -- Character placement boxes and notes used to recreate the placeholder composition after refresh.
 placeholder_placements_json TEXT NOT NULL DEFAULT '[]',
 video_history_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS game_edges (id TEXT PRIMARY KEY, game_id TEXT NOT NULL, source_node_id TEXT NOT NULL, target_node_id TEXT NOT NULL, option_text TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 1, conditions_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE, FOREIGN KEY (source_node_id) REFERENCES game_nodes(id) ON DELETE CASCADE, FOREIGN KEY (target_node_id) REFERENCES game_nodes(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS game_tasks (id TEXT PRIMARY KEY, game_id TEXT NOT NULL, type TEXT NOT NULL, resource_id TEXT, status TEXT NOT NULL, input_snapshot_json TEXT, result_json TEXT, error_message TEXT, progress INTEGER NOT NULL DEFAULT 0, stage TEXT NOT NULL DEFAULT '', poll_attempts INTEGER NOT NULL DEFAULT 0, poll_lease_token TEXT, poll_lease_until TEXT, next_poll_at TEXT, provider_task_id TEXT, created_at TEXT NOT NULL, started_at TEXT, completed_at TEXT, FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS game_sessions (id TEXT PRIMARY KEY, game_id TEXT NOT NULL, current_node_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active', path_json TEXT NOT NULL DEFAULT '[]',
-- State flags written by earlier choices and evaluated by conditional choices after later DAG merges.
state_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE, FOREIGN KEY (current_node_id) REFERENCES game_nodes(id));
CREATE TABLE IF NOT EXISTS game_choice_events (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, game_id TEXT NOT NULL, source_node_id TEXT NOT NULL, edge_id TEXT NOT NULL, target_node_id TEXT NOT NULL, option_text TEXT NOT NULL, selected_at TEXT NOT NULL, FOREIGN KEY (session_id) REFERENCES game_sessions(id) ON DELETE CASCADE, FOREIGN KEY (game_id) REFERENCES interactive_games(id) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_drama_assets_drama_id ON drama_assets(drama_id);
CREATE INDEX IF NOT EXISTS idx_drama_shots_drama_id ON drama_shots(drama_id);
CREATE INDEX IF NOT EXISTS idx_generation_tasks_drama_id ON generation_tasks(drama_id);
CREATE INDEX IF NOT EXISTS idx_voice_generation_tasks_status ON voice_generation_tasks(status,created_at);
CREATE INDEX IF NOT EXISTS idx_game_assets_game_id ON game_assets(game_id);
CREATE INDEX IF NOT EXISTS idx_game_nodes_game_id ON game_nodes(game_id);
CREATE INDEX IF NOT EXISTS idx_game_edges_game_id ON game_edges(game_id);
CREATE INDEX IF NOT EXISTS idx_game_tasks_game_id ON game_tasks(game_id);
CREATE INDEX IF NOT EXISTS idx_game_sessions_game_id ON game_sessions(game_id);
CREATE INDEX IF NOT EXISTS idx_game_choice_events_session_id ON game_choice_events(session_id);
CREATE INDEX IF NOT EXISTS idx_shot_versions_shot_id ON drama_shot_versions(shot_id, version_no);
CREATE INDEX IF NOT EXISTS idx_prompt_templates_scope ON prompt_templates(scope, name, active);
"#;
