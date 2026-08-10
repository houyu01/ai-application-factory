//! Byte-range media responses for the Tauri video and image elements.

use std::{fs, path::PathBuf};

use tauri::http::Response;

/// Serve an app-owned media file with the range support required by WebView video decoding.
pub fn response(path: Option<PathBuf>, range_header: Option<&str>) -> Response<Vec<u8>> {
    let Some(path) = path else {
        return text_response(404, "Media not found");
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return text_response(404, "Media not found"),
    };
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    match byte_range(range_header, bytes.len()) {
        Ok(Some((start, end))) => media_response(
            206,
            &content_type,
            bytes[start..=end].to_vec(),
            Some(format!("bytes {start}-{end}/{}", bytes.len())),
        ),
        Ok(None) => media_response(200, &content_type, bytes, None),
        Err(()) => media_response(
            416,
            "text/plain; charset=utf-8",
            Vec::new(),
            Some(format!("bytes */{}", bytes.len())),
        ),
    }
}

fn byte_range(value: Option<&str>, total: usize) -> Result<Option<(usize, usize)>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    let range = value.strip_prefix("bytes=").ok_or(())?;
    if range.contains(',') {
        return Err(());
    }
    let (start, end) = range.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<usize>().map_err(|_| ())?;
        if suffix == 0 || total == 0 {
            return Err(());
        }
        return Ok(Some((total.saturating_sub(suffix), total - 1)));
    }
    let start = start.parse::<usize>().map_err(|_| ())?;
    if start >= total {
        return Err(());
    }
    let end = if end.is_empty() {
        total - 1
    } else {
        end.parse::<usize>().map_err(|_| ())?.min(total - 1)
    };
    (start <= end).then_some((start, end)).ok_or(()).map(Some)
}

fn media_response(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    content_range: Option<String>,
) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(status)
        .header("content-type", content_type)
        .header("content-length", body.len().to_string())
        .header("accept-ranges", "bytes")
        .header("access-control-allow-origin", "*");
    if let Some(content_range) = content_range {
        builder = builder.header("content-range", content_range);
    }
    builder.body(body).expect("media response is valid")
}

fn text_response(status: u16, body: &str) -> Response<Vec<u8>> {
    media_response(
        status,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{byte_range, response};

    #[test]
    fn parses_video_byte_ranges_for_incremental_decoding() {
        assert_eq!(byte_range(Some("bytes=0-1023"), 2_048), Ok(Some((0, 1023))));
        assert_eq!(
            byte_range(Some("bytes=1024-"), 2_048),
            Ok(Some((1024, 2047)))
        );
        assert_eq!(
            byte_range(Some("bytes=-512"), 2_048),
            Ok(Some((1536, 2047)))
        );
    }

    #[test]
    fn rejects_unsatisfiable_video_byte_ranges() {
        assert_eq!(byte_range(Some("bytes=2048-"), 2_048), Err(()));
    }

    #[test]
    fn returns_partial_media_with_canvas_safe_headers() {
        let path = std::env::temp_dir().join(format!("video-range-{}.mp4", crate::value::new_id()));
        fs::write(&path, b"0123456789").expect("write test video");
        let result = response(Some(path.clone()), Some("bytes=2-5"));
        let _ = fs::remove_file(path);

        assert_eq!(result.status(), 206);
        assert_eq!(result.headers()["content-range"], "bytes 2-5/10");
        assert_eq!(result.headers()["access-control-allow-origin"], "*");
        assert_eq!(result.body(), b"2345");
    }
}
