#!/bin/bash

cat ../AGENTS.md > ../CLAUDE.md
cat ../AGENTS.md > ../.github/copilot-instructions.md

# Generate copilot instructions
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Commit", "**")' ../.github/copilot-instructions.md > ../.github/instructions/commit.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Language Server Protocol", "crates/mq-lsp/**/*.rs")' ../.github/copilot-instructions.md > ../.github/instructions/lsp.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Testing", "tests/**/*.rs,crates/**/tests/**/*.rs")' ../.github/copilot-instructions.md > ../.github/instructions/testing.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Markdown Parser/Utility Coding Rules", "crates/mq-markdown/**/*.rs")' ../.github/copilot-instructions.md > ../.github/instructions/markdown.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Documentation", "**/README.md,docs/**/*.md")' ../.github/copilot-instructions.md > ../.github/instructions/docs.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("Rust Crate", "crates/**/*.rs")' ../.github/copilot-instructions.md > ../.github/instructions/rust-crate.instructions.md
mq 'include "ai_rules" | nodes | gen_ai_rules_with_apply_to("CLI Tool Coding Rules", "crates/mq-cli/**/*.rs")' ../.github/copilot-instructions.md > ../.github/instructions/cli.instructions.md
