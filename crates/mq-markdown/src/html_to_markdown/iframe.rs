/// Detects the embed platform from an iframe `src` URL and returns
/// `(description, canonical_url)` for known platforms.
///
/// Detection is performed with lightweight string operations — no regex dependency.
pub(super) fn detect_embed(src: &str) -> Option<(String, String)> {
    detect_youtube(src)
        .or_else(|| detect_vimeo(src))
        .or_else(|| detect_instagram(src))
        .or_else(|| detect_dailymotion(src))
        .or_else(|| detect_twitch(src))
        .or_else(|| detect_spotify(src))
        .or_else(|| detect_vk(src))
        .or_else(|| detect_google_docs(src))
}

/// Returns the first path segment that immediately follows `needle` in `src`.
/// Stops at the next `/`, `?`, `#`, or end of string.
fn path_segment_after<'a>(src: &'a str, needle: &str) -> Option<&'a str> {
    let start = src.find(needle).map(|i| i + needle.len())?;
    let rest = &src[start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let segment = &rest[..end];
    if segment.is_empty() { None } else { Some(segment) }
}

/// Returns the value of a URL query parameter by name.
fn query_param<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    let query_start = src.find('?').map(|i| i + 1)?;
    for part in src[query_start..].split('&') {
        if let Some((k, v)) = part.split_once('=')
            && k == key
        {
            return Some(v);
        }
    }
    None
}

fn detect_youtube(src: &str) -> Option<(String, String)> {
    if !src.contains("youtube.com/embed/") && !src.contains("youtube-nocookie.com/embed/") {
        return None;
    }
    let video_id = path_segment_after(src, "/embed/")?;
    Some((
        "YouTube Video".to_string(),
        format!("https://www.youtube.com/watch?v={}", video_id),
    ))
}

fn detect_vimeo(src: &str) -> Option<(String, String)> {
    if !src.contains("player.vimeo.com/video/") {
        return None;
    }
    let video_id = path_segment_after(src, "/video/")?;
    Some(("Vimeo Video".to_string(), format!("https://vimeo.com/{}", video_id)))
}

fn detect_instagram(src: &str) -> Option<(String, String)> {
    if !src.contains("instagram.com/p/") {
        return None;
    }
    let post_id = path_segment_after(src, "/p/")?;
    Some((
        "Instagram Post".to_string(),
        format!("https://www.instagram.com/p/{}/", post_id),
    ))
}

fn detect_dailymotion(src: &str) -> Option<(String, String)> {
    if !src.contains("dailymotion.com/embed/video/") {
        return None;
    }
    let video_id = path_segment_after(src, "/embed/video/")?;
    Some((
        "Dailymotion Video".to_string(),
        format!("https://www.dailymotion.com/video/{}", video_id),
    ))
}

fn detect_twitch(src: &str) -> Option<(String, String)> {
    if src.contains("player.twitch.tv") {
        if let Some(channel) = query_param(src, "channel") {
            return Some((
                "Twitch Stream".to_string(),
                format!("https://www.twitch.tv/{}", channel),
            ));
        }
        if let Some(video) = query_param(src, "video") {
            return Some((
                "Twitch Video".to_string(),
                format!("https://www.twitch.tv/videos/{}", video),
            ));
        }
    }
    if src.contains("clips.twitch.tv")
        && let Some(clip) = query_param(src, "clip")
    {
        return Some(("Twitch Clip".to_string(), format!("https://clips.twitch.tv/{}", clip)));
    }
    None
}

fn detect_spotify(src: &str) -> Option<(String, String)> {
    if !src.contains("open.spotify.com/embed/") {
        return None;
    }
    let kind = path_segment_after(src, "/embed/")?;
    let needle = format!("/embed/{}/", kind);
    let id = path_segment_after(src, &needle)?;
    let label = match kind {
        "track" => "Spotify Track",
        "album" => "Spotify Album",
        "playlist" => "Spotify Playlist",
        "episode" => "Spotify Episode",
        "show" => "Spotify Podcast",
        _ => "Spotify",
    };
    Some((label.to_string(), format!("https://open.spotify.com/{}/{}", kind, id)))
}

fn detect_vk(src: &str) -> Option<(String, String)> {
    if !src.contains("vk.com/video_ext.php") {
        return None;
    }
    let oid = query_param(src, "oid")?;
    let vid = query_param(src, "id")?;
    Some(("VK Video".to_string(), format!("https://vk.com/video{}_{}", oid, vid)))
}

fn detect_google_docs(src: &str) -> Option<(String, String)> {
    if !src.contains("docs.google.com") {
        return None;
    }
    let doc_id = path_segment_after(src, "/d/")?;
    if src.contains("/presentation/") {
        Some((
            "Google Slides".to_string(),
            format!("https://docs.google.com/presentation/d/{}/", doc_id),
        ))
    } else if src.contains("/document/") {
        Some((
            "Google Docs".to_string(),
            format!("https://docs.google.com/document/d/{}/", doc_id),
        ))
    } else if src.contains("/spreadsheets/") {
        Some((
            "Google Sheets".to_string(),
            format!("https://docs.google.com/spreadsheets/d/{}/", doc_id),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("https://www.youtube.com/embed/dQw4w9WgXcQ", Some(("YouTube Video", "https://www.youtube.com/watch?v=dQw4w9WgXcQ")))]
    #[case("https://www.youtube.com/embed/dQw4w9WgXcQ?autoplay=1", Some(("YouTube Video", "https://www.youtube.com/watch?v=dQw4w9WgXcQ")))]
    #[case("https://www.youtube-nocookie.com/embed/abc123XYZ", Some(("YouTube Video", "https://www.youtube.com/watch?v=abc123XYZ")))]
    #[case("https://player.vimeo.com/video/123456789", Some(("Vimeo Video", "https://vimeo.com/123456789")))]
    #[case("https://www.instagram.com/p/B1BKr9Wo8YX/embed/", Some(("Instagram Post", "https://www.instagram.com/p/B1BKr9Wo8YX/")))]
    #[case("https://www.dailymotion.com/embed/video/x7zflst", Some(("Dailymotion Video", "https://www.dailymotion.com/video/x7zflst")))]
    #[case("https://player.twitch.tv/?channel=monstercat&parent=example.com", Some(("Twitch Stream", "https://www.twitch.tv/monstercat")))]
    #[case("https://player.twitch.tv/?video=v123456789&parent=example.com", Some(("Twitch Video", "https://www.twitch.tv/videos/v123456789")))]
    #[case("https://clips.twitch.tv/embed?clip=FuriousObliqueDonutDansGame", Some(("Twitch Clip", "https://clips.twitch.tv/FuriousObliqueDonutDansGame")))]
    #[case("https://open.spotify.com/embed/track/4iV5W9uYEdYUVa79Axb7Rh", Some(("Spotify Track", "https://open.spotify.com/track/4iV5W9uYEdYUVa79Axb7Rh")))]
    #[case("https://open.spotify.com/embed/album/1DFixLWuPkv3KT3TnV35m3", Some(("Spotify Album", "https://open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3")))]
    #[case("https://open.spotify.com/embed/playlist/37i9dQZF1DXcBWIGoYBM5M", Some(("Spotify Playlist", "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M")))]
    #[case("https://open.spotify.com/embed/episode/64Q5MbCIFuuEaCf6fxJblG", Some(("Spotify Episode", "https://open.spotify.com/episode/64Q5MbCIFuuEaCf6fxJblG")))]
    #[case("https://vk.com/video_ext.php?oid=-49423435&id=456245092&hash=e1611aefe899c4f8", Some(("VK Video", "https://vk.com/video-49423435_456245092")))]
    #[case("https://docs.google.com/presentation/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/embed", Some(("Google Slides", "https://docs.google.com/presentation/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/")))]
    #[case("https://docs.google.com/document/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/preview", Some(("Google Docs", "https://docs.google.com/document/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/")))]
    #[case("https://docs.google.com/spreadsheets/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/htmlview", Some(("Google Sheets", "https://docs.google.com/spreadsheets/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/")))]
    #[case("https://example.com/embed/video/abc", None)]
    #[case("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None)]
    fn test_detect_embed(#[case] src: &str, #[case] expected: Option<(&str, &str)>) {
        let result = detect_embed(src);
        match (result, expected) {
            (Some((desc, url)), Some((exp_desc, exp_url))) => {
                assert_eq!(desc, exp_desc);
                assert_eq!(url, exp_url);
            }
            (None, None) => {}
            (got, exp) => panic!("expected {:?}, got {:?}", exp, got),
        }
    }
}
