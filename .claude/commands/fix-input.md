---
allowed-tools: Read(*.rs,*.md,*.ts,*.tsx,*.js,*.toml,*.json)
description: "Analyzes and fixes content or code provided as input."
---

# Fix Input

Analyzes and fixes content or code provided as input, such as code snippets, configuration files, or documentation.

## Input Target

$ARGUMENTS will be used to specify the content to fix:
- If $ARGUMENTS contains code snippets, analyze and fix syntax or logic errors
- If $ARGUMENTS contains file content, identify and resolve issues
- If $ARGUMENTS contains configuration, validate and correct settings
- If $ARGUMENTS contains documentation, improve clarity and accuracy
- Supports multiple inputs or descriptions separated by spaces

## What it does

- Analyzes provided input content using $ARGUMENTS
- Identifies syntax errors, logic issues, or improvement opportunities
- Fixes content following mq project conventions and best practices
- Ensures Rust code uses miette for error handling
- Validates configuration against project standards
- Improves documentation clarity and accuracy

## How to use

Run the fix-input command with the content to analyze:

```
/fix-input "fn parse_markdown(input: &str) -> Result<Document> { panic!("not implemented") }"
/fix-input "Invalid TOML configuration in Cargo.toml"
/fix-input "README.md section needs better examples"
/fix-input "Error handling in this function is incorrect"
```

The command will automatically:
1. Parse $ARGUMENTS to understand the input content
2. Analyze the provided content for issues
3. Read related project files if context is needed
4. Provide fixed version following project standards
5. Explain what was changed and why

## Fix Process

The fix process follows these steps:

1. **Input Analysis**
   - Parse $ARGUMENTS to extract the content to fix
   - Identify the type of content (code, config, docs, etc.)
   - Determine the scope and context of the input

2. **Issue Identification**
   - Analyze syntax and logic errors
   - Check against project conventions
   - Identify improvement opportunities
   - Consider edge cases and error handling

3. **Solution Implementation**
   - Fix syntax and logic errors
   - Apply project coding conventions
   - Improve error handling with miette
   - Enhance clarity and maintainability

4. **Validation & Explanation**
   - Ensure fixes follow project standards
   - Validate against similar code in the project
   - Provide clear explanation of changes
   - Suggest additional improvements if relevant

## Content Types Handled

- Rust code snippets and functions
- Configuration files (TOML, JSON, YAML)
- Documentation and README content
- Test code and examples
- Command-line usage examples
- API documentation
- Error messages and user feedback
- Build scripts and automation

The fix will provide the corrected content along with a detailed explanation of what was changed, why it was changed, and how it aligns with the mq project standards.