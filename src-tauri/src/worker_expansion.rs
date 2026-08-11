//! Resumable long-drama screenplay expansion before the existing decomposition task persists shots.

use std::thread;

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    skills,
    value::GENERATING,
};

use super::{retry::immediate_language_retry_delay, DurableWorker};

#[path = "worker_screenplay_stream.rs"]
mod screenplay_stream;
#[path = "worker_story_bible.rs"]
mod story_bible;
#[path = "worker_expansion_support.rs"]
pub(crate) mod support;

use support::*;

const EPISODES_PER_INSTALLMENT: i64 = 5;
const MAX_INSTALLMENTS: i64 = 30;
const RESEARCH_TOPICS: [&str; 4] = [
    "主角成长与身份反转的节奏",
    "多集追更钩子与中段升级结构",
    "人物关系、情感线与对手线的交叉推进",
    "终局回收伏笔与成功结局的结构",
];
const FIRST_APPEARANCE_RULE: &str = "人物首次出场规则：每位人物在全剧第一次清晰出场时，紧随首个动作写一次“【人物首次出场：当前名字｜人物描述：身份或关系、年龄/形态、2～3项稳定外观、当下情绪或动作】”。当前名字使用该时刻剧情正在使用的姓名、称谓或化名；同一人物后续改名、使用别称或再次出现时不得重复标记。描述必须具体可视化，不能使用泛称。";

impl DurableWorker {
    /// Expand or resume the numbered screenplay owned by a bootstrap task, checkpointing every completed batch.
    pub(super) fn prepare_screenplay_for_decomposition(
        &self,
        task_id: &str,
        project_id: &str,
        project: &Value,
    ) -> AppResult<String> {
        let source = clean(project["script"].as_str().unwrap_or_default());
        if source.is_empty() {
            return Err(AppError::BadRequest("剧本内容不能为空".to_owned()));
        }
        let minimum = project["expanded_script_min_chars"]
            .as_i64()
            .unwrap_or(5_000)
            .max(1) as usize;
        let maximum = project["expanded_script_max_chars"]
            .as_i64()
            .unwrap_or(10_000)
            .max(minimum as i64) as usize;
        let episodes = target_episode_count(project)?;
        let mut existing = clean(project["expanded_script"].as_str().unwrap_or_default());
        if !existing.is_empty() && !resumable(&existing) {
            existing.clear();
        }
        if long_ready(&existing, minimum, episodes) {
            self.repository
                .update_drama_task_progress(task_id, 60, "已读取已保存的扩写剧本")?;
            return Ok(existing);
        }
        if long_ready(&source, minimum, episodes) {
            return Ok(source);
        }
        let selected_model = project["language_model"].as_str();
        if !self.providers.text_configured("language", selected_model)? {
            return Err(AppError::BadRequest(format!(
                "未配置可调用的语言模型，无法执行 {episodes} 集长剧扩写。请先在配置页保存语言模型的 endpoint、API Key 和可选模型。"
            )));
        }
        self.ensure_expansion_active(task_id)?;
        let mut snapshot = self.expansion_snapshot(task_id)?;
        let source_excerpt = source_excerpt(&source);
        let enable_web = crate::value::bool_value(&project["enable_web_search"]);
        let mut bible = snapshot_text(&snapshot, "story_bible");
        if bible.is_empty() {
            let research = self.research_frameworks(
                task_id,
                project,
                selected_model,
                &source_excerpt,
                enable_web,
            )?;
            self.repository.update_drama_task_progress(
                task_id,
                8,
                &format!("正在生成 {episodes} 集故事圣经"),
            )?;
            bible = self.stream_story_bible(
                task_id,
                project,
                selected_model,
                &source_excerpt,
                &research,
                enable_web,
                &mut snapshot,
            )?;
            snapshot.insert("story_bible".to_owned(), json!(bible));
            self.repository
                .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
        } else {
            self.repository.update_drama_task_progress(
                task_id,
                8,
                &format!("已读取已保存的 {episodes} 集故事圣经"),
            )?;
        }
        let mut screenplay = existing;
        let mut installment =
            (episode_sections(&screenplay).len() as i64 + EPISODES_PER_INSTALLMENT - 1)
                / EPISODES_PER_INSTALLMENT
                + 1;
        while episode_sections(&screenplay).len() < episodes as usize
            && installment <= MAX_INSTALLMENTS
        {
            self.ensure_expansion_active(task_id)?;
            let start = episode_sections(&screenplay)
                .iter()
                .map(|item| item.number)
                .max()
                .unwrap_or(0)
                + 1;
            let end = (start + EPISODES_PER_INSTALLMENT - 1).min(episodes);
            let per_episode_target = (maximum / episodes as usize).max(1);
            let batch_target = per_episode_target * (end - start + 1) as usize;
            let stage = format!("正在扩写第{start:03}至第{end:03}集");
            self.repository.update_drama_task_progress(
                task_id,
                expansion_progress(char_count(&screenplay), minimum),
                &stage,
            )?;
            let chunk = self.write_installment(
                task_id,
                &mut snapshot,
                project,
                selected_model,
                &source_excerpt,
                &bible,
                &screenplay,
                installment,
                start,
                end,
                per_episode_target,
                batch_target,
                enable_web,
            )?;
            let chunk = fit_installment(chunk, start, end)?;
            if chunk.is_empty() {
                return Err(AppError::External(format!(
                    "扩写剧本第 {installment} 节没有返回有效内容"
                )));
            }
            screenplay = join_screenplay(&screenplay, &chunk);
            snapshot.insert("expanded_script_preview".to_owned(), json!(screenplay));
            self.repository
                .set_expanded_screenplay(project_id, &screenplay)?;
            self.repository
                .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
            self.repository.update_drama_task_progress(
                task_id,
                expansion_progress(char_count(&screenplay), minimum),
                &format!("扩写剧本已保存（{} 字）", char_count(&screenplay)),
            )?;
            installment += 1;
        }
        validate_screenplay(&screenplay, episodes)?;
        self.repository.update_drama_task_progress(
            task_id,
            60,
            &format!("扩写剧本已保存（{} 字）", char_count(&screenplay)),
        )?;
        Ok(screenplay)
    }

    /// Append exactly one creator-triggered continuation while keeping the existing decomposition graph intact.
    pub(super) fn continue_expanded_screenplay(
        &self,
        task_id: &str,
        project_id: &str,
    ) -> AppResult<()> {
        let project = self.repository.raw_drama(project_id)?;
        let source = clean(project["script"].as_str().unwrap_or_default());
        let existing = clean(project["expanded_script"].as_str().unwrap_or_default());
        if source.is_empty() {
            return Err(AppError::BadRequest("剧本内容不能为空".to_owned()));
        }
        if existing.is_empty() {
            return Err(AppError::BadRequest("尚无可继续扩写的剧本内容".to_owned()));
        }
        let minimum = project["expanded_script_min_chars"]
            .as_i64()
            .unwrap_or(5_000)
            .max(1) as usize;
        let maximum = project["expanded_script_max_chars"]
            .as_i64()
            .unwrap_or(10_000)
            .max(minimum as i64) as usize;
        self.ensure_expansion_active(task_id)?;
        self.repository
            .update_drama_task_progress(task_id, 5, "正在继续扩写剧本")?;
        let selected_model = project["language_model"].as_str();
        let mut snapshot = self.expansion_snapshot(task_id)?;
        let source_excerpt = source_excerpt(&source);
        let enable_web = crate::value::bool_value(&project["enable_web_search"]);
        let mut bible = snapshot_text(&snapshot, "story_bible");
        if bible.is_empty() {
            bible = self.generate_story_bible(
                &project,
                selected_model,
                &source_excerpt,
                "",
                enable_web,
            )?;
            snapshot.insert("story_bible".to_owned(), json!(bible));
            self.repository
                .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
        }
        let chunk = self.write_continuation(
            task_id,
            &mut snapshot,
            selected_model,
            &source_excerpt,
            &bible,
            &existing,
            minimum,
            maximum,
            enable_web,
        )?;
        if chunk.is_empty() {
            return Err(AppError::External("继续扩写没有返回有效内容".to_owned()));
        }
        let screenplay = join_screenplay(&existing, &chunk);
        snapshot.insert("expanded_script_preview".to_owned(), json!(screenplay));
        snapshot.insert("continuation_complete".to_owned(), json!(true));
        self.repository
            .set_expanded_screenplay(project_id, &screenplay)?;
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot))?;
        self.repository.finish_drama_task(task_id, crate::value::SUCCEEDED, Some(json!({"original_script_length":char_count(&source),"expanded_script_length":char_count(&screenplay)})), None)?;
        Ok(())
    }

    fn write_installment(
        &self,
        task_id: &str,
        snapshot: &mut Map<String, Value>,
        project: &Value,
        model: Option<&str>,
        source: &str,
        bible: &str,
        screenplay: &str,
        installment: i64,
        start: i64,
        end: i64,
        target_chars: usize,
        batch_target: usize,
        web: bool,
    ) -> AppResult<String> {
        let writer = skills::drama_skill(
            "script_writer",
            json!({"story_bible":clip_chars(bible,6_000),"episode_card":format!("只写第{start:03}集至第{end:03}集；每集均有独立标题、完整场景动作对白和本集结尾钩子，不能跨范围补写。"),"scene_plan":"按因果推进剧情，避免总结、重复和跳过关键冲突。","style_requirements":"中文影视剧本格式，包含场景、动作、对白、情绪和结尾钩子。"}),
        )?;
        let prompt = format!("请直接续写长篇短剧正文，不要解释、不要写创作说明。必须是具体场景、动作、对白和情绪推进，而不是梗概或重复前文。\n项目创建配置：目标剧集数={}集；扩写剧本总字数目标为{}至{}字；每个分镜剧本文字不超过{}字；风格={}；题材={}；画幅={}；分辨率={}；分镜元素约束={}。\n这是第 {installment} 批。只输出第{start:03}至第{end:03}集，每集以单独一行“【第001集：集名】”开始；每集建议约{target_chars}个中文字符，本批建议约{batch_target}个中文字符。字数是写作目标，不要因接近上限删减已有剧情，优先保证剧情完整和剧集结构连续。\n{FIRST_APPEARANCE_RULE}\n写作技能：{}\n故事圣经：\n{}\n原始剧本：\n{source}\n上一节末尾（仅用于衔接）：\n{}", target_episode_count(project)?, project["expanded_script_min_chars"], project["expanded_script_max_chars"], project["shot_script_max_chars"], project["style"].as_str().unwrap_or("真人风格"), project["theme"].as_str().unwrap_or("都市"), project["ratio"].as_str().unwrap_or("9:16"), project["resolution"].as_str().unwrap_or("720p"), project["shot_constraints"], writer["instruction"].as_str().unwrap_or_default(), clip_chars(bible,6_000), clip_chars(screenplay,2_400));
        self.stream_screenplay_installment(
            task_id,
            snapshot,
            screenplay,
            model,
            &prompt,
            web,
            &format!("正在扩写第{start:03}至第{end:03}集"),
            project["expanded_script_min_chars"]
                .as_i64()
                .unwrap_or(5_000)
                .max(1) as usize,
        )
        .map(|value| clean(&value))
    }

    fn write_continuation(
        &self,
        task_id: &str,
        snapshot: &mut Map<String, Value>,
        model: Option<&str>,
        source: &str,
        bible: &str,
        existing: &str,
        minimum_chars: usize,
        target_chars: usize,
        web: bool,
    ) -> AppResult<String> {
        let current_chars = char_count(existing);
        let prompt = format!("请直接续写长篇短剧正文，不要解释、不要写创作说明。必须是具体场景、动作、对白和情绪推进，而不是梗概或重复前文。\n完整剧本的目标字数为 {minimum_chars} 至 {target_chars} 个中文字符；当前已保存 {current_chars} 个字。请根据剩余剧情补足、收束或完善，使完整剧本尽量落在这个配置范围内。字数范围是写作目标，不是程序硬性截断；接近上限时可以短写结尾，但不要重复已有内容。\n{FIRST_APPEARANCE_RULE}\n故事圣经：\n{}\n原始剧本：\n{source}\n上一节末尾（仅用于衔接）：\n{}", clip_chars(bible,6_000), clip_chars(existing,2_400));
        self.stream_screenplay_installment(
            task_id,
            snapshot,
            existing,
            model,
            &prompt,
            web,
            "正在继续扩写剧本",
            target_chars,
        )
        .map(|value| clean(&value))
    }

    fn research_frameworks(
        &self,
        task_id: &str,
        project: &Value,
        model: Option<&str>,
        source: &str,
        web: bool,
    ) -> AppResult<String> {
        if !web {
            return Ok(String::new());
        }
        let mut notes = Vec::new();
        for (index, topic) in RESEARCH_TOPICS.iter().enumerate() {
            self.ensure_expansion_active(task_id)?;
            self.repository.update_drama_task_progress(
                task_id,
                5,
                &format!(
                    "正在联网研究同类故事框架（{}/{}）：{}",
                    index + 1,
                    RESEARCH_TOPICS.len(),
                    topic
                ),
            )?;
            let skill = skills::drama_skill(
                "story_framework_researcher",
                json!({"premise":source,"topic":topic}),
            )?;
            let prompt = format!("研究技能：{}\n请使用 web_search 查询与下列创意在类型、受众或叙事节奏上相近的公开小说、短剧或影视作品介绍；四轮合计覆盖3至4个不同作品。只总结可迁移的抽象叙事框架。不要复述原文、不要输出作品人物名、专有剧情或长引用。重点：{topic}。\n用户创意：\n{source}", skill["instruction"].as_str().unwrap_or_default());
            let note = clean(&self.completion_retry(
                model,
                &prompt,
                web,
                &format!("联网叙事框架研究：{topic}"),
            )?);
            if !note.is_empty() {
                notes.push(format!("【{topic}】{}", clip_chars(&note, 2_000)));
            }
        }
        if notes.len() < 3 {
            return Err(AppError::External(format!(
                "联网同类故事框架研究不足 3 条（当前 {} 条），无法开始{}集剧本扩写",
                notes.len(),
                target_episode_count(project)?
            )));
        }
        Ok(notes.join("\n\n"))
    }

    fn completion_retry(
        &self,
        model: Option<&str>,
        prompt: &str,
        web: bool,
        stage: &str,
    ) -> AppResult<String> {
        let system = format!(
            "{}\n\n你是短剧长篇编剧，严格遵守用户项目的集数、字数、编号与原创要求。",
            skills::drama_agent_system()?
        );
        for attempt in 1..=3 {
            match self
                .providers
                .complete_with_web_search("language", model, &system, prompt, web)
            {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => return Err(AppError::BadRequest("未配置可调用的语言模型".to_owned())),
                Err(error) if immediate_language_retry_delay(&error, attempt).is_some() => {
                    thread::sleep(
                        immediate_language_retry_delay(&error, attempt)
                            .expect("retry delay was checked"),
                    )
                }
                Err(error) => {
                    return Err(AppError::External(format!(
                        "{stage}请求语言模型失败（已尝试 {attempt} 次）：{error}"
                    )))
                }
            }
        }
        unreachable!()
    }

    pub(super) fn ensure_expansion_active(&self, task_id: &str) -> AppResult<()> {
        if self.repository.get_drama_task(task_id)?["status"].as_str() == Some(GENERATING) {
            Ok(())
        } else {
            Err(AppError::BadRequest("剧本扩写已取消".to_owned()))
        }
    }

    fn expansion_snapshot(&self, task_id: &str) -> AppResult<Map<String, Value>> {
        Ok(self.repository.get_drama_task(task_id)?["input_snapshot"]
            .as_object()
            .cloned()
            .unwrap_or_default())
    }
}

fn snapshot_text(snapshot: &Map<String, Value>, key: &str) -> String {
    snapshot
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_expansion_snapshot_has_no_story_bible() {
        assert_eq!(snapshot_text(&Map::new(), "story_bible"), "");
    }
}
