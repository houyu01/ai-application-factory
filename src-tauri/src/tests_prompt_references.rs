//! Regression coverage for placing rich-prompt references next to their semantic fields.

use serde_json::json;

use crate::planner;

#[test]
fn fallback_prompt_places_scene_character_and_prop_references_in_their_sections() {
    let project = json!({"style":"真人风格","ratio":"9:16"});
    let shot = json!({"duration_seconds":10,"original_text":"林岩在旧居找到信物。"});
    let assets = vec![
        json!({"id":"scene","type":"scene","name":"旧居"}),
        json!({"id":"character","type":"character","name":"林岩"}),
        json!({"id":"prop","type":"prop","name":"信物"}),
    ];

    let prompt = planner::prompt_text(&planner::fallback_rich_prompt(&project, &shot, &assets));

    assert!(prompt.contains("场景：@图1（旧居）\n角色：@图2（林岩）\n道具：@图3（信物）"));
    assert!(!prompt.contains("自动匹配参考图"));
}
