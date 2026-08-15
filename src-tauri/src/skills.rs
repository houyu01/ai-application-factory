//! Standard `SKILL.md` discovery and execution envelopes for local domain agents.

use std::{collections::BTreeMap, path::Path, sync::OnceLock};

#[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
use std::{fs, path::PathBuf};

use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

static REGISTRY: OnceLock<SkillRegistry> = OnceLock::new();

const EMBEDDED_SKILLS: &[(&str, &str)] = &[
    (
        "drama/asset_prompt_generator/SKILL.md",
        include_str!("../resources/skills/drama/asset_prompt_generator/SKILL.md"),
    ),
    (
        "drama/continuity_checker/SKILL.md",
        include_str!("../resources/skills/drama/continuity_checker/SKILL.md"),
    ),
    (
        "drama/episode_planner/SKILL.md",
        include_str!("../resources/skills/drama/episode_planner/SKILL.md"),
    ),
    (
        "drama/episode_summarizer/SKILL.md",
        include_str!("../resources/skills/drama/episode_summarizer/SKILL.md"),
    ),
    (
        "drama/premise_expander/SKILL.md",
        include_str!("../resources/skills/drama/premise_expander/SKILL.md"),
    ),
    (
        "drama/scene_planner/SKILL.md",
        include_str!("../resources/skills/drama/scene_planner/SKILL.md"),
    ),
    (
        "drama/script_decomposer/SKILL.md",
        include_str!("../resources/skills/drama/script_decomposer/SKILL.md"),
    ),
    (
        "drama/script_writer/SKILL.md",
        include_str!("../resources/skills/drama/script_writer/SKILL.md"),
    ),
    (
        "drama/shot_prompt_generator/SKILL.md",
        include_str!("../resources/skills/drama/shot_prompt_generator/SKILL.md"),
    ),
    (
        "drama/story_bible_generator/SKILL.md",
        include_str!("../resources/skills/drama/story_bible_generator/SKILL.md"),
    ),
    (
        "drama/story_framework_researcher/SKILL.md",
        include_str!("../resources/skills/drama/story_framework_researcher/SKILL.md"),
    ),
    (
        "interactive_game/interactive_branch_planner/SKILL.md",
        include_str!("../resources/skills/interactive_game/interactive_branch_planner/SKILL.md"),
    ),
];

#[derive(Clone, Debug)]
struct SkillDefinition {
    name: String,
    description: String,
    agent: String,
    metadata: BTreeMap<String, String>,
    instruction: String,
}

#[derive(Clone, Debug, Default)]
struct SkillRegistry {
    by_name: BTreeMap<String, SkillDefinition>,
}

/// Load the bundle-owned skill directory before services or workers begin processing tasks.
#[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
pub(crate) fn initialize(directory: PathBuf) -> AppResult<()> {
    let registry = SkillRegistry::load(&directory)?;
    install(registry)
}

/// Load compile-time skill content on mobile, where APK assets are URI-backed rather than files.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn initialize_embedded() -> AppResult<()> {
    install(SkillRegistry::load_embedded()?)
}

fn install(registry: SkillRegistry) -> AppResult<()> {
    REGISTRY
        .set(registry)
        .map_err(|_| AppError::BadRequest("技能注册表已初始化".to_owned()))
}

/// Match Python `BaseSkill.execute`: retain the skill name, domain agent, formatted instruction, and arguments.
pub(crate) fn drama_skill(name: &str, arguments: Value) -> AppResult<Value> {
    let skill = registry().named_for_agent(name, "drama")?;
    Ok(json!({
        "skill": name,
        "agent": "drama",
        "instruction": drama_instruction(skill, &arguments)?,
        "arguments": arguments,
    }))
}

/// Return the system context injected by Python's `DramaAgent` before each provider request.
pub(crate) fn drama_agent_system() -> AppResult<String> {
    let summary = registry()
        .for_agent("drama")
        .into_iter()
        .map(|skill| format!("- {}: {}", skill.name, skill.description))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "You are the drama domain agent. Use the available skills when a task matches them.\nAvailable skills:\n{summary}"
    ))
}

/// Return the interactive-game skill envelope, which is defined but not yet invoked by its deterministic planner.
#[allow(dead_code)]
pub(crate) fn game_branch_skill(arguments: Value) -> AppResult<Value> {
    let skill = registry().named_for_agent("interactive_branch_planner", "interactive_game")?;
    Ok(json!({
        "skill": skill.name,
        "agent": skill.agent,
        "instruction": skill.instruction,
        "arguments": arguments,
    }))
}

fn registry() -> &'static SkillRegistry {
    REGISTRY.get_or_init(|| {
        SkillRegistry::load_embedded()
            .expect("embedded SKILL.md files must be valid before a worker starts")
    })
}

#[cfg(test)]
fn development_skill_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/skills")
}

fn drama_instruction(skill: &SkillDefinition, arguments: &Value) -> AppResult<String> {
    let instruction = match skill.name.as_str() {
        "premise_expander" => format_instruction(
            &skill.instruction,
            arguments,
            &[
                "episode_count",
                "target_min_chars",
                "target_max_chars",
                "shot_script_max_chars",
            ],
        )?,
        "script_decomposer" => {
            format_instruction(&skill.instruction, arguments, &["shot_script_max_chars"])?
        }
        "shot_prompt_generator" if arguments["prompt_template_version"].as_str() == Some("v2") => {
            let original = skill.metadata.get("prompt_template_v1").ok_or_else(|| {
                AppError::BadRequest(
                    "shot_prompt_generator 缺少 prompt_template_v1 元数据".to_owned(),
                )
            })?;
            let replacement = skill.metadata.get("prompt_template_v2").ok_or_else(|| {
                AppError::BadRequest(
                    "shot_prompt_generator 缺少 prompt_template_v2 元数据".to_owned(),
                )
            })?;
            skill.instruction.replace(original, replacement)
        }
        "story_bible_generator" => format_instruction(
            &skill.instruction,
            arguments,
            &["episode_count", "format_requirements"],
        )?,
        _ => skill.instruction.clone(),
    };
    Ok(instruction)
}

fn format_instruction(template: &str, arguments: &Value, fields: &[&str]) -> AppResult<String> {
    let mut instruction = template.to_owned();
    for field in fields {
        let value = arguments
            .get(*field)
            .ok_or_else(|| AppError::BadRequest(format!("skill 参数缺少 {field}")))?;
        let replacement = match *field {
            "target_min_chars" | "target_max_chars" | "shot_script_max_chars" => {
                grouped_number(value)?
            }
            "episode_count" => value
                .as_i64()
                .ok_or_else(|| AppError::BadRequest(format!("skill 参数 {field} 必须是整数")))?
                .to_string(),
            _ => value
                .as_str()
                .ok_or_else(|| AppError::BadRequest(format!("skill 参数 {field} 必须是文本")))?
                .to_owned(),
        };
        instruction = instruction.replace(&format!("{{{field}}}"), &replacement);
    }
    Ok(instruction)
}

fn grouped_number(value: &Value) -> AppResult<String> {
    let number = value
        .as_i64()
        .ok_or_else(|| AppError::BadRequest("skill 数值参数必须是整数".to_owned()))?;
    let raw = number.to_string();
    let mut result = String::new();
    for (index, character) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            result.push(',');
        }
        result.push(character);
    }
    Ok(result.chars().rev().collect())
}

impl SkillRegistry {
    #[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
    fn load(directory: &Path) -> AppResult<Self> {
        let mut files = Vec::new();
        collect_skill_files(directory, &mut files)?;
        if files.is_empty() {
            return Err(AppError::BadRequest(format!(
                "未找到 SKILL.md：{}",
                directory.display()
            )));
        }
        let mut by_name = BTreeMap::new();
        for file in files {
            let definition = parse_skill(&fs::read_to_string(&file)?, &file)?;
            if by_name
                .insert(definition.name.clone(), definition)
                .is_some()
            {
                return Err(AppError::BadRequest(format!(
                    "skill 名称重复：{}",
                    file.display()
                )));
            }
        }
        Ok(Self { by_name })
    }

    fn load_embedded() -> AppResult<Self> {
        let mut by_name = BTreeMap::new();
        for (relative_path, source) in EMBEDDED_SKILLS {
            let path = Path::new(relative_path);
            let definition = parse_skill(source, path)?;
            if by_name
                .insert(definition.name.clone(), definition)
                .is_some()
            {
                return Err(AppError::BadRequest(format!(
                    "skill 名称重复：{}",
                    path.display()
                )));
            }
        }
        Ok(Self { by_name })
    }

    fn named_for_agent(&self, name: &str, agent: &str) -> AppResult<&SkillDefinition> {
        let skill = self
            .by_name
            .get(name)
            .ok_or_else(|| AppError::BadRequest(format!("未知的 {agent} skill：{name}")))?;
        if skill.agent == agent {
            Ok(skill)
        } else {
            Err(AppError::BadRequest(format!(
                "skill {name} 不属于 {agent} agent"
            )))
        }
    }

    fn for_agent(&self, agent: &str) -> Vec<&SkillDefinition> {
        self.by_name
            .values()
            .filter(|skill| skill.agent == agent)
            .collect()
    }
}

#[cfg(any(test, not(any(target_os = "android", target_os = "ios"))))]
fn collect_skill_files(directory: &Path, files: &mut Vec<PathBuf>) -> AppResult<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_skill_files(&path, files)?;
        } else if path.file_name().is_some_and(|name| name == "SKILL.md") {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_skill(source: &str, path: &Path) -> AppResult<SkillDefinition> {
    let source = source.replace("\r\n", "\n");
    let source = source.strip_prefix("---\n").ok_or_else(|| {
        AppError::BadRequest(format!(
            "SKILL.md 缺少 YAML frontmatter：{}",
            path.display()
        ))
    })?;
    let (frontmatter, body) = source.split_once("\n---\n").ok_or_else(|| {
        AppError::BadRequest(format!("SKILL.md frontmatter 未结束：{}", path.display()))
    })?;
    let mut name = None;
    let mut description = None;
    let mut agent = None;
    let mut metadata = BTreeMap::new();
    let mut in_metadata = false;
    for line in frontmatter.lines() {
        if line == "metadata:" {
            in_metadata = true;
            continue;
        }
        if !line.starts_with(' ') {
            in_metadata = false;
            if let Some((key, value)) = line.split_once(':') {
                match key {
                    "name" => name = Some(yaml_scalar(value)),
                    "description" => description = Some(yaml_scalar(value)),
                    _ => {}
                }
            }
        } else if in_metadata {
            if let Some((key, value)) = line.trim().split_once(':') {
                metadata.insert(key.to_owned(), yaml_scalar(value));
                if key == "agent" {
                    agent = Some(yaml_scalar(value));
                }
            }
        }
    }
    let required = |value: Option<String>, field: &str| {
        value.filter(|value| !value.is_empty()).ok_or_else(|| {
            AppError::BadRequest(format!("SKILL.md 缺少 {field}：{}", path.display()))
        })
    };
    Ok(SkillDefinition {
        name: required(name, "name")?,
        description: required(description, "description")?,
        agent: required(agent, "metadata.agent")?,
        metadata,
        instruction: body.trim_end_matches('\n').to_owned(),
    })
}

fn yaml_scalar(value: &str) -> String {
    value.trim().trim_matches('"').trim_matches('\'').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_skill_markdown_exposes_frontmatter_and_body() {
        let registry = SkillRegistry::load(&development_skill_directory()).expect("skill registry");
        let skill = registry
            .named_for_agent("premise_expander", "drama")
            .expect("premise skill");
        assert_eq!(
            skill.description,
            "将一句话创意扩展为可持续创作的故事核心设定。"
        );
        assert!(skill.instruction.contains("{target_min_chars}"));
        assert_eq!(registry.for_agent("drama").len(), 11);
    }

    #[test]
    fn embedded_skills_load_without_a_runtime_resource_directory() {
        let registry = SkillRegistry::load_embedded().expect("embedded skill registry");
        assert_eq!(registry.by_name.len(), 12);
        assert_eq!(registry.for_agent("drama").len(), 11);
        registry
            .named_for_agent("interactive_branch_planner", "interactive_game")
            .expect("interactive game skill");
    }

    #[test]
    fn v2_prompt_template_uses_skill_metadata_without_changing_the_contract() {
        let registry = SkillRegistry::load(&development_skill_directory()).expect("skill registry");
        let skill = registry
            .named_for_agent("shot_prompt_generator", "drama")
            .expect("shot prompt skill");
        let instruction = drama_instruction(skill, &json!({"prompt_template_version":"v2"}))
            .expect("v2 instruction");
        assert!(instruction.contains("只生成 1 个完整的连续长镜头"));
        assert!(!instruction.contains("生成 2～3 个连续镜头"));
    }
}
