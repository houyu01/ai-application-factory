//! API router regression coverage for browser-encoded desktop request paths.

use url::Url;

use crate::api::path_parts;

#[test]
fn encoded_shot_id_is_decoded_before_routing() {
    let url = Url::parse(
        "http://desktop.local/projects/470e2008-05ab-4678-a5c7-5e08041003b1/shots/470e2008-05ab-4678-a5c7-5e08041003b1%3Ashot%3A1%3A1/videos",
    )
    .expect("valid desktop request URL");

    assert_eq!(
        path_parts(&url).expect("decode route path"),
        vec![
            "projects",
            "470e2008-05ab-4678-a5c7-5e08041003b1",
            "shots",
            "470e2008-05ab-4678-a5c7-5e08041003b1:shot:1:1",
            "videos",
        ]
    );
}
