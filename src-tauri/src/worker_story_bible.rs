//! Story-bible generation with durable previews for the long-drama bootstrap flow.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    skills,
};

use super::{support::*, DurableWorker};

const PREVIEW_WRITE_INTERVAL: Duration = Duration::from_millis(250);
const PREVIEW_WRITE_MIN_BYTES: usize = 96;

impl DurableWorker {
    /// Stream the first story-bible request, checkpointing output so the creation screen can show live model text.
    pub(super) fn stream_story_bible(
        &self,
        task_id: &str,
        project: &Value,
        model: Option<&str>,
        source: &str,
        research: &str,
        web: bool,
        snapshot: &mut Map<String, Value>,
    ) -> AppResult<String> {
        let episodes = target_episode_count(project)?;
        let prompt = story_bible_prompt(project, source, research)?;
        let mut preview = String::new();
        let mut saved_bytes = 0;
        let mut last_saved = Instant::now() - PREVIEW_WRITE_INTERVAL;
        snapshot.insert("story_bible_preview".to_owned(), json!(preview));
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
        let streamed = self.providers.complete_with_web_search_stream(
            "language",
            model,
            &story_bible_system()?,
            &prompt,
            web,
            |delta| {
                preview.push_str(delta);
                let due = preview.len().saturating_sub(saved_bytes) >= PREVIEW_WRITE_MIN_BYTES
                    || last_saved.elapsed() >= PREVIEW_WRITE_INTERVAL;
                if due {
                    self.persist_story_bible_preview(task_id, episodes, snapshot, &preview)?;
                    saved_bytes = preview.len();
                    last_saved = Instant::now();
                }
                Ok(())
            },
        );
        if saved_bytes != preview.len() {
            self.persist_story_bible_preview(task_id, episodes, snapshot, &preview)?;
        }
        streamed
            .map(|value| clean(&value.unwrap_or_default()).if_empty(source))
            .map_err(story_bible_error)
    }

    /// Generate a story bible for a later continuation where no creation-page preview is active.
    pub(super) fn generate_story_bible(
        &self,
        project: &Value,
        model: Option<&str>,
        source: &str,
        research: &str,
        web: bool,
    ) -> AppResult<String> {
        let prompt = story_bible_prompt(project, source, research)?;
        self.completion_retry(model, &prompt, web, "故事大纲")
            .map(|value| clean(&value).if_empty(source))
    }

    fn persist_story_bible_preview(
        &self,
        task_id: &str,
        episodes: i64,
        snapshot: &mut Map<String, Value>,
        preview: &str,
    ) -> AppResult<()> {
        self.ensure_expansion_active(task_id)?;
        snapshot.insert("story_bible_preview".to_owned(), json!(preview));
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
        self.repository.update_drama_task_progress(
            task_id,
            8,
            &format!(
                "正在生成 {episodes} 集故事圣经（已接收 {} 字）",
                char_count(preview)
            ),
        )
    }
}

fn story_bible_prompt(project: &Value, source: &str, research: &str) -> AppResult<String> {
    let episodes = target_episode_count(project)?;
    let premise = skills::drama_skill(
        "premise_expander",
        json!({"premise":source,"genre":project["theme"].as_str().unwrap_or("短剧"),"target_audience":"短剧观众","episode_count":episodes,"target_min_chars":project["expanded_script_min_chars"],"target_max_chars":project["expanded_script_max_chars"],"shot_script_max_chars":project["shot_script_max_chars"]}),
    )?;
    let format = format!("项目创建配置：目标剧集数={episodes}集；扩写剧本总字数={}至{}字；每个分镜剧本文字不超过{}字；风格={}；题材={}；画幅={}；分辨率={}；分镜元素约束={}。\n必须规划{episodes}集，按连续篇章组织。逐集给出集号、集名、核心冲突、人物推进、结尾钩子和衔接状态；不要复述或模仿任何检索作品。", project["expanded_script_min_chars"], project["expanded_script_max_chars"], project["shot_script_max_chars"], project["style"].as_str().unwrap_or("真人风格"), project["theme"].as_str().unwrap_or("都市"), project["ratio"].as_str().unwrap_or("9:16"), project["resolution"].as_str().unwrap_or("720p"), project["shot_constraints"]);
    let bible = skills::drama_skill(
        "story_bible_generator",
        json!({"premise":source,"expanded_concept":premise["instruction"],"episode_count":episodes,"format_requirements":format}),
    )?;
    Ok(format!("请为长篇短剧扩写建立紧凑故事圣经和分集推进表。保留原稿中的明确人物、事件、设定和情感走向；补齐连续的冲突、反转、伏笔和结局，不要写正文剧本。\n{format}\n联网同类框架研究（只可借鉴抽象叙事结构，禁止复写作品内容）：\n{research}\n创意扩写技能：{}\n故事圣经技能：{}\n原始剧本：\n{source}", premise["instruction"].as_str().unwrap_or_default(), bible["instruction"].as_str().unwrap_or_default()))
}

fn story_bible_system() -> AppResult<String> {
    Ok(format!(
        "{}\n\n你是短剧长篇编剧，严格遵守用户项目的集数、字数、编号与原创要求。",
        skills::drama_agent_system()?
    ))
}

fn story_bible_error(error: AppError) -> AppError {
    match error {
        AppError::External(message) if message.contains("请求超时") => AppError::External(
            "故事圣经生成超时（3 分钟内未收到完整结果），请检查语言模型连接后重新生成".to_owned(),
        ),
        other => other,
    }
}
