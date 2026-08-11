//! Reference-audio assembly for Seedance and Wan video request adapters.

use serde_json::{json, Value};

use super::video_state::unique;

/// Build Ark's multimodal content array and include source audio only for Seedance 2.x requests.
pub(super) fn ark_content(
    prompt: &str,
    references: &[String],
    reference_audio: &[Option<String>],
    reference_video: Option<&str>,
    supports_audio: bool,
) -> Vec<Value> {
    let mut content = vec![json!({"type":"text","text":prompt})];
    if let Some(video) = reference_video.filter(|value| !value.is_empty()) {
        content
            .push(json!({"type":"video_url","video_url":{"url":video},"role":"reference_video"}));
    }
    for (index, image) in unique(references).into_iter().enumerate() {
        content
            .push(json!({"type":"image_url","image_url":{"url":image},"role":"reference_image"}));
        if supports_audio {
            if let Some(audio) = reference_audio.get(index).and_then(|value| value.as_ref()) {
                content.push(
                    json!({"type":"audio_url","audio_url":{"url":audio},"role":"reference_audio"}),
                );
            }
        }
    }
    content
}

/// Preserve the audio paired with each first occurrence after image URLs are deduplicated.
pub(super) fn unique_reference_audio(
    references: &[String],
    reference_audio: &[Option<String>],
) -> Vec<Option<String>> {
    let mut seen = std::collections::HashSet::new();
    references
        .iter()
        .enumerate()
        .filter_map(|(index, url)| {
            seen.insert(url)
                .then(|| reference_audio.get(index).cloned().unwrap_or(None))
        })
        .collect()
}

/// Seedance 2.x accepts audio URL content items with the reference-audio role.
pub(super) fn supports_ark_reference_audio(model: &str) -> bool {
    let model = model.to_lowercase();
    model.contains("seedance-2") || model.contains("seedance_2")
}

#[cfg(test)]
mod tests {
    use super::{ark_content, unique_reference_audio};

    #[test]
    fn ark_reference_audio_stays_next_to_its_character_image() {
        let content = ark_content(
            "微调提示",
            &["https://example.com/reference.png".to_owned()],
            &[Some("https://example.com/voice.mp3".to_owned())],
            Some("https://example.com/source.mp4"),
            true,
        );
        assert_eq!(content[1]["type"], "video_url");
        assert_eq!(content[2]["role"], "reference_image");
        assert_eq!(content[3]["role"], "reference_audio");
    }

    #[test]
    fn reference_audio_stays_paired_with_the_first_unique_image() {
        let audio = unique_reference_audio(
            &[
                "https://example.com/a.png".to_owned(),
                "https://example.com/a.png".to_owned(),
                "https://example.com/b.png".to_owned(),
            ],
            &[
                Some("https://example.com/a.mp3".to_owned()),
                None,
                Some("https://example.com/b.mp3".to_owned()),
            ],
        );
        assert_eq!(
            audio,
            vec![
                Some("https://example.com/a.mp3".to_owned()),
                Some("https://example.com/b.mp3".to_owned())
            ]
        );
    }
}
