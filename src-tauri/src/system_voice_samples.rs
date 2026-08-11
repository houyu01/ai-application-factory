//! Bundled, provider-generated MP3 assets for the fixed system voice catalog.
//!
//! Database seeding assigns each system preset its stable local media URL. `MediaStore` materializes
//! these bytes into the app-data media directory on startup so Settings can play them without a model call.

/// One immutable, packaged voice sample paired with its stable local-media filename.
pub(crate) struct SystemVoiceSample {
    /// Stable `voice_presets.id` used by the character selector and catalog seed data.
    pub(crate) id: &'static str,
    /// App-owned media filename copied into the local media directory at startup.
    pub(crate) media_id: &'static str,
    /// Provider-generated MP3 data compiled into the desktop binary.
    pub(crate) bytes: &'static [u8],
}

macro_rules! sample {
    ($id:literal) => {
        SystemVoiceSample {
            id: $id,
            media_id: concat!("system-voice-", $id, ".mp3"),
            bytes: include_bytes!(concat!("../resources/voice_samples/", $id, ".mp3")),
        }
    };
}

const SAMPLES: &[SystemVoiceSample] = &[
    sample!("broken_whisper_resilient_female"),
    sample!("cold_boss_male"),
    sample!("cool_career_newcomer_male"),
    sample!("soft_puppy_boyfriend_male"),
    sample!("sickly_gloomy_yandere_male"),
    sample!("ruthless_old_fox_male"),
    sample!("arrogant_genius_male"),
    sample!("cool_abstinent_detective_female"),
    sample!("warm_older_brother_male"),
    sample!("sweet_cold_yandere_male"),
    sample!("cold_royal_sister_female"),
    sample!("strong_female_lead"),
    sample!("mature_warm_goddess_female"),
    sample!("sweet_fox_tease_female"),
];

/// Return every pre-generated sample copied by `MediaStore` during desktop startup.
pub(crate) fn all() -> &'static [SystemVoiceSample] {
    SAMPLES
}

/// Return the system sample URL stored in `voice_presets.audio_url` for one fixed catalog id.
pub(crate) fn audio_url(voice_id: &str) -> Option<String> {
    sample_for_voice(voice_id).map(|sample| format!("/api/media/{}", sample.media_id))
}

/// Report whether a voice id belongs to the product-shipped catalog rather than a creator preview.
pub(crate) fn is_system_voice(voice_id: &str) -> bool {
    sample_for_voice(voice_id).is_some()
}

/// Report whether a local-media filename belongs to an immutable system sample.
pub(crate) fn is_system_media(media_id: &str) -> bool {
    SAMPLES.iter().any(|sample| sample.media_id == media_id)
}

fn sample_for_voice(voice_id: &str) -> Option<&'static SystemVoiceSample> {
    SAMPLES.iter().find(|sample| sample.id == voice_id)
}
