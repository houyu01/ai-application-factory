//! First-frame visual locks for provider-bound video generation inputs.

use std::collections::HashMap;

/// Put a selected first frame at reference index one while retaining rich-prompt marker alignment.
pub(super) fn prioritize_first_frame(
    images: &mut Vec<String>,
    markers: &mut HashMap<i64, usize>,
    first_frame: &str,
) -> usize {
    let position = images.iter().position(|image| image == first_frame);
    match position {
        Some(0) => 1,
        Some(position) => {
            let frame = images.remove(position);
            images.insert(0, frame);
            let original_index = position + 1;
            for index in markers.values_mut() {
                if *index == original_index {
                    *index = 1;
                } else if *index < original_index {
                    *index += 1;
                }
            }
            1
        }
        None => {
            images.insert(0, first_frame.to_owned());
            for index in markers.values_mut() {
                *index += 1;
            }
            1
        }
    }
}

/// Make a user-selected first frame the non-negotiable visual source of truth for a video task.
pub(super) fn first_frame_instruction(index: usize) -> String {
    format!(
        "首帧图锁定（最高优先级，必须遵守）：@图{index} 是本分镜已明确选择的首帧图，是唯一视觉基准和生成起点，不是可选参考；它的要求优先于任何文字描述或其他参考图。\n视频第 1 帧必须严格复现 @图{index} 中全部可见人物的同一身份、脸部五官、年龄、性别、发型、体型、服装、配饰与表情；同一场景的空间结构、建筑或布景、陈设、光线、色调和时间氛围；以及全部道具的种类、数量、外观、材质、颜色、位置和人物关系。\n之后镜头只能从该首帧自然连续地运动和演化：禁止改用、替换、合并或凭空新增/删除任何人物、场景或道具；禁止改变人物身份或外观、场景地点或道具归属。如文本与首帧图冲突，以首帧图为准。"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{first_frame_instruction, prioritize_first_frame};

    #[test]
    fn selected_first_frame_is_first_and_locks_all_visual_elements() {
        let mut images = vec![
            "scene.png".to_owned(),
            "first.png".to_owned(),
            "prop.png".to_owned(),
        ];
        let mut markers = HashMap::from([(1, 1), (2, 2), (3, 3)]);

        assert_eq!(
            prioritize_first_frame(&mut images, &mut markers, "first.png"),
            1
        );
        assert_eq!(images, ["first.png", "scene.png", "prop.png"]);
        assert_eq!(markers, HashMap::from([(1, 2), (2, 1), (3, 3)]));

        let instruction = first_frame_instruction(1);
        for text in [
            "最高优先级",
            "人物",
            "场景",
            "道具",
            "禁止改用",
            "以首帧图为准",
        ] {
            assert!(instruction.contains(text), "missing {text}");
        }
    }
}
