//! The dialect a document is read in, and the constructs each has.

/// The dialect a document is read in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Flavor {
    /// `CommonMark` with GFM, math, frontmatter and the Obsidian syntax of the enabled features.
    #[default]
    Markdown,
    /// MDX: `CommonMark` without indented code, HTML and autolinks, but with JSX, expressions and ESM.
    Mdx,
}

impl Flavor {
    /// Indented code blocks.
    pub(super) fn has_indented_code(self) -> bool {
        self == Self::Markdown
    }

    /// HTML blocks, raw inline HTML and autolinks in angle brackets.
    pub(super) fn has_html(self) -> bool {
        self == Self::Markdown
    }

    /// GFM: tables, task list items, footnotes, strikethrough and autolink literals.
    pub(super) fn has_gfm(self) -> bool {
        self == Self::Markdown
    }

    /// Math blocks and inline math.
    pub(super) fn has_math(self) -> bool {
        self == Self::Markdown
    }

    /// Wikilinks and embeds, when their features are enabled.
    #[cfg(any(feature = "wikilink", feature = "embed"))]
    pub(super) fn has_obsidian(self) -> bool {
        self == Self::Markdown
    }

    /// JSX tags, expressions and `import`/`export` statements.
    pub(super) fn has_jsx(self) -> bool {
        self == Self::Mdx
    }

    /// Whether code and math spans drop the indentation of their continuation lines, as the lines of a
    /// paragraph do. In MDX the content of a span is kept as written.
    pub(super) fn strips_span_indent(self) -> bool {
        self == Self::Markdown
    }
}
