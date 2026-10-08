//! HTML for the Obsidian syntax: wikilinks, embeds and callouts.
//!
//! The classes and attributes are the ones Obsidian writes, so styles made for it apply. A link
//! target is used as the URL as written, because the files of a vault are not known here.

#[cfg(feature = "callout")]
use super::super::callout::Header;
#[cfg(feature = "callout")]
use super::super::tree::Block;
#[cfg(feature = "embed")]
use super::SAFE_PROTOCOL_SRC;
use super::{Html, encode};
#[cfg(any(feature = "wikilink", feature = "embed"))]
use super::{SAFE_PROTOCOL_HREF, sanitize_with_protocols};

/// What Obsidian shows for a link without text: `note#Heading` is `note > Heading`.
#[cfg(any(feature = "wikilink", feature = "embed"))]
fn shown_target(target: &str) -> String {
    match target.split_once('#') {
        Some(("", heading)) => heading.to_string(),
        Some((page, heading)) => format!("{page} > {heading}"),
        None => target.to_string(),
    }
}

/// How an embedded file is shown, by its extension.
#[cfg(feature = "embed")]
enum Media {
    Image,
    Audio,
    Video,
    Pdf,
    Note,
}

#[cfg(feature = "embed")]
fn media(target: &str) -> Media {
    let path = target.split(['#', '?']).next().unwrap_or_default();
    let Some((_, extension)) = path.rsplit_once('.') else {
        return Media::Note;
    };
    match extension.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "avif" => Media::Image,
        "mp3" | "wav" | "m4a" | "ogg" | "3gp" | "flac" => Media::Audio,
        "mp4" | "webm" | "ogv" | "mov" | "mkv" => Media::Video,
        "pdf" => Media::Pdf,
        _ => Media::Note,
    }
}

/// The width and the height from a display like `100` or `100x200`.
#[cfg(feature = "embed")]
fn size(display: &str) -> Option<(&str, Option<&str>)> {
    let digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    match display.split_once('x') {
        Some((width, height)) if digits(width) && digits(height) => Some((width, Some(height))),
        None if digits(display) => Some((display, None)),
        _ => None,
    }
}

/// The alt text and the size from a display like `alt`, `100`, or `alt|100`.
#[cfg(feature = "embed")]
fn alt_and_size(display: &str) -> (Option<&str>, Option<(&str, Option<&str>)>) {
    match display.rsplit_once('|') {
        Some((alt, last)) => match size(last) {
            Some(dimensions) => (Some(alt), Some(dimensions)),
            None => (Some(display), None),
        },
        None => match size(display) {
            Some(dimensions) => (None, Some(dimensions)),
            None => (Some(display), None),
        },
    }
}

impl Html {
    /// `<a>` to the target, which is plain text inside another link.
    #[cfg(any(feature = "wikilink", feature = "embed"))]
    fn internal_link(&mut self, class: &str, target: &str, shown: &str) {
        if self.in_link {
            encode(&mut self.out, shown);
            return;
        }
        self.out.push_str(&format!(
            "<a class=\"{class}\" href=\"{}\" data-href=\"",
            sanitize_with_protocols(target, &SAFE_PROTOCOL_HREF)
        ));
        encode(&mut self.out, target);
        self.out.push_str("\">");
        encode(&mut self.out, shown);
        self.out.push_str("</a>");
    }

    /// `[[target|text]]`.
    #[cfg(feature = "wikilink")]
    pub(super) fn wikilink(&mut self, target: &str, text: Option<&str>) {
        let shown = text.map_or_else(|| shown_target(target), str::to_string);
        self.internal_link("internal-link", target, &shown);
    }

    /// `![[target|display]]`: an image, audio, video or PDF where the browser can show the file, and a
    /// link where it cannot, which is a note.
    #[cfg(feature = "embed")]
    pub(super) fn embed(&mut self, target: &str, display: Option<&str>) {
        let kind = media(target);
        if matches!(kind, Media::Note) {
            let shown = display.map_or_else(|| shown_target(target), str::to_string);
            return self.internal_link("internal-link embed", target, &shown);
        }

        let src = sanitize_with_protocols(target, &SAFE_PROTOCOL_SRC);
        let (alt, dimensions) = display.map_or((None, None), alt_and_size);
        let mut attributes = String::new();
        if let Some((width, height)) = dimensions {
            attributes.push_str(&format!(" width=\"{width}\""));
            if let Some(height) = height {
                attributes.push_str(&format!(" height=\"{height}\""));
            }
        }

        match kind {
            Media::Image => {
                self.out.push_str(&format!("<img src=\"{src}\" alt=\""));
                encode(&mut self.out, alt.filter(|alt| !alt.is_empty()).unwrap_or(target));
                self.out.push_str(&format!("\"{attributes} />"));
            }
            Media::Audio => self.out.push_str(&format!("<audio controls src=\"{src}\"></audio>")),
            Media::Video => self
                .out
                .push_str(&format!("<video controls src=\"{src}\"{attributes}></video>")),
            Media::Pdf => self
                .out
                .push_str(&format!("<embed type=\"application/pdf\" src=\"{src}\"{attributes} />")),
            Media::Note => {}
        }
    }

    /// A callout: `<details>` when it can be folded, and a `<div>` otherwise. `head` is what remains of
    /// the first paragraph after the header line, and `rest` the other blocks of the quote.
    #[cfg(feature = "callout")]
    pub(super) fn callout(&mut self, header: &Header, head: &[Block], rest: &[Block]) {
        let kind = header.kind.to_lowercase();
        let title = header.title.clone().unwrap_or_else(|| {
            let mut chars = kind.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        });

        self.line_ending_if_needed();
        let mut data = String::new();
        encode(&mut data, &kind);
        let closing = if header.fold.is_some() {
            let open = if header.fold == Some('+') { " open" } else { "" };
            self.out
                .push_str(&format!("<details class=\"callout\" data-callout=\"{data}\"{open}>"));
            self.line_ending();
            self.out.push_str("<summary class=\"callout-title\">");
            encode(&mut self.out, &title);
            self.out.push_str("</summary>");
            "</details>"
        } else {
            self.out
                .push_str(&format!("<div class=\"callout\" data-callout=\"{data}\">"));
            self.line_ending();
            self.out.push_str("<div class=\"callout-title\">");
            encode(&mut self.out, &title);
            self.out.push_str("</div>");
            "</div>"
        };

        if !head.is_empty() || !rest.is_empty() {
            self.line_ending();
            self.out.push_str("<div class=\"callout-content\">");
            self.tight.push(false);
            self.blocks(head);
            self.blocks(rest);
            self.tight.pop();
            self.line_ending_if_needed();
            self.out.push_str("</div>");
        }
        self.line_ending();
        self.out.push_str(closing);
    }
}
