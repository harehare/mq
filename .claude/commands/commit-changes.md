---
allowed-tools: Bash(git add,git switch,git commit,git status), Read(*.rs,*.md,*.ts,*.tsx,*.js,*.toml,*.json), Glob(*), Grep(*)
description: "Reviews modified code, creates an appropriate branch, and commits changes with descriptive messages."
---

# Commit Changes

Reviews all modified code in the repository, creates an appropriate feature branch based on the changes, and commits the modifications with a well-crafted commit message following the project conventions.

## What it does

1. **Code Review**: Analyzes all modified files for quality, adherence to project standards, and potential issues
2. **Branch Creation**: Generates an appropriate branch name based on the type and scope of changes
3. **Staging**: Adds all modified files to the git staging area
4. **Commit**: Creates a commit with a descriptive message following the project's commit message conventions

## How to use

Simply run the command to review and commit all current changes:

```
/commit-changes
```

The command will automatically:
1. Check `git status` to identify modified files
2. Review the modified code for quality and compliance
3. Generate an appropriate branch name (e.g., `feature/add-parser-improvements`, `fix/error-handling-bug`)
4. Create and switch to the new branch using `git switch -c <branch-name>`
5. Stage all changes with `git add .`
6. Create a commit with a message following the format: `<type>(<scope>): <description>`

## Branch Naming Convention

Branches are created following this pattern:
- `feat/<description>` - For new features or enhancements
- `fix/<description>` - For bug fixes
- `refactor/<description>` - For code refactoring
- `docs/<description>` - For documentation changes
- `test/<description>` - For test-related changes
- `perf/<description>` - For performance improvements

## Commit Message Format

Commits follow the project convention:
```
<type>(<scope>): <description>

[optional body if needed]
```

Types include:
- ✨ feat: New feature
- 🐛 fix: Bug fix
- 📝 docs: Documentation changes
- 💄 style: Code style changes
- ♻️ refactor: Refactoring
- ⚡ perf: Performance improvements
- ✅ test: Adding or modifying tests
- 📦 build: Build system changes
- 👷 ci: CI configuration changes

## Pre-commit Validation

Before committing, the command ensures:

- Code follows Rust formatting standards
- No obvious syntax errors or issues
- Changes align with project conventions from CLAUDE.md
- Commit message is clear and descriptive

## Safety Features

- Only uses safe git operations (`git add`, `git switch`, `git commit`)
- Creates descriptive branch names to maintain clean git history
- Follows established project patterns and conventions
