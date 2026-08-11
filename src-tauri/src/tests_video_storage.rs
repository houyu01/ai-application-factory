//! Regression coverage for the completed-model-video storage policy.

use std::fs;

use crate::{db::Database, media::MediaStore, repository::Repository, value::new_id};

#[test]
fn local_storage_keeps_the_provider_video_url_without_a_duplicate_download() {
    let root = std::env::temp_dir().join(format!("ai-video-storage-test-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let media = MediaStore::new(repository).expect("media store");
    let provider_url = "https://videos.example.com/generated.mp4?token=temporary";

    assert_eq!(
        media
            .save_generated_video_url(provider_url)
            .expect("keep provider URL"),
        provider_url
    );
    assert!(fs::read_dir(root.join("media"))
        .expect("media directory")
        .next()
        .is_none());
    fs::remove_dir_all(root).expect("remove test data");
}
