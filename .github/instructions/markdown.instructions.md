---
applyTo: "crates/mq-markdown/**/*.rs"
---

# Markdown Parser/Utility Coding Rules

- All Markdown parsing and manipulation logic must reside in `mq-markdown`.
- Write tests for all parsing and transformation functions.
- Ensure robust handling of edge cases in Markdown syntax.
- Document all public APIs and provide usage examples in doc comments.
- Avoid panics on malformed input; return descriptive errors using `miette`.
- Keep the API surface minimal and focused on Markdown processing.

