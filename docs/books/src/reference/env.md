# Environment variables

Environment variables can be referenced using $XXX syntax, where XXX represents the name of the environment variable. For example:

- `$PATH` - References the PATH environment variable
- `$HOME` - References the HOME environment variable
- `$USER` - References the current user's username

This syntax is commonly used in shell scripts and configuration files to access system-level environment variables.

## Color Configuration

### `NO_COLOR`

When set to a non-empty value, disables all colored output regardless of the `-C` flag. This follows the [NO_COLOR](https://no-color.org/) standard.

```sh
# Disable colored output
NO_COLOR=1 mq -C '.h' README.md
```

### `MQ_COLORS`

Customizes the colors used when `-C` (color output) is enabled. The format is a colon-separated list of `key=value` pairs, where each value is a semicolon-separated list of [SGR (Select Graphic Rendition)](https://en.wikipedia.org/wiki/ANSI_escape_code#SGR) parameters.

```sh
# Make headings bold red, code blocks blue
export MQ_COLORS="heading=1;31:code=34"
mq -C '.h' README.md
```

Only the specified keys are overridden; unspecified keys use the default colors. Invalid entries are silently ignored.

#### Available Keys

| Key           | Description                   | Default                 |
| ------------- | ----------------------------- | ----------------------- |
| `heading`     | Headings (`#`, `##`, etc.)    | bold cyan (`1;36`)      |
| `code`        | Fenced code blocks            | green (`32`)            |
| `code_inline` | Inline code                   | green (`32`)            |
| `emphasis`    | Italic text (`*text*`)        | italic yellow (`3;33`)  |
| `strong`      | Bold text (`**text**`)        | bold (`1`)              |
| `link`        | Links (`[text](url)`)         | underline blue (`4;34`) |
| `link_url`    | Link URLs                     | blue (`34`)             |
| `image`       | Images (`![alt](url)`)        | magenta (`35`)          |
| `blockquote`  | Blockquote markers (`>`)      | dim (`2`)               |
| `delete`      | Strikethrough (`~~text~~`)    | red dim (`31;2`)        |
| `hr`          | Horizontal rules (`---`)      | dim (`2`)               |
| `html`        | Inline HTML                   | dim (`2`)               |
| `frontmatter` | YAML/TOML frontmatter         | dim (`2`)               |
| `list`        | List markers (`-`, `*`, `1.`) | yellow (`33`)           |
| `table`       | Table separators              | dim (`2`)               |
| `math`        | Math expressions (`$...$`)    | green (`32`)            |

#### Common SGR Codes

| Code | Effect    |
| ---- | --------- |
| `0`  | Reset     |
| `1`  | Bold      |
| `2`  | Dim       |
| `3`  | Italic    |
| `4`  | Underline |
| `31` | Red       |
| `32` | Green     |
| `33` | Yellow    |
| `34` | Blue      |
| `35` | Magenta   |
| `36` | Cyan      |
| `37` | White     |
