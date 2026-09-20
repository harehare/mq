# Turn command output into structured data

Goal: Parse the text printed by CLI commands such as `ps`, `df`, `ls -l` and `git log` into arrays of dicts, then filter it or render it as a Markdown table or JSON.

Prerequisites: The [cmd.mq](https://github.com/harehare/cmd.mq) extension module. Copy `cmd.mq` into your module directory, or import it over HTTP with `--allow-http-import` and `import "github.com/harehare/cmd.mq"`. Pipe the command's output in and read it with `-I raw`, which gives the query the whole text as a single string.

## Query

The processes using the most memory, as a Markdown table:

```bash
$ ps aux | mq -I raw 'import "cmd" | import "csv" | cmd::ps_parse | sort_by(fn(p): -p["mem_percent"];) | map(fn(p): {"pid": p["pid"], "mem_percent": p["mem_percent"], "command": p["command"]};) | slice(0, 2) | csv::csv_to_markdown_table()'
```

## Input (`ps aux`)

```
USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND
root         1  0.0  0.1 168936 11840 ?        Ss   Sep19   0:03 /sbin/init splash
alice     1204 12.5  3.2 2450112 262144 ?      Sl   09:15   1:42 /usr/bin/firefox --new-window
alice     1377  0.3  0.8 812340  65536 pts/0   Ss   09:16   0:00 -bash
```

## Output

```markdown
| pid | mem_percent | command |
| --- | --- | --- |
| 1204 | 3.2 | /usr/bin/firefox --new-window |
| 1377 | 0.8 | -bash |
```

## Summarize `git log`

`git_log_parse` reads the default `git log` format into `commit`, `author`, `date` and `message`, plus `merge` and `refs` when present. This turns it into a Markdown list of short hashes and subjects:

```bash
$ git log -2 | mq -I raw 'import "cmd" | cmd::git_log_parse | map(fn(c): "- " + slice(c["commit"], 0, 7) + " " + first(split(c["message"], "\n")) + " (" + c["author"] + ")";) | join("\n")'
```

```markdown
- 9fceb02 fix(parser): handle empty input (Alice <alice@example.com>)
- 1a2b3c4 feat(cli): add --watch flag (Bob <bob@example.com>)
```

## Run the command from mq

`cmd::run(cmd, args)` runs the command and parses its output in one call, so no pipe or `-I raw` is needed. It uses the `system` function, which requires `--allow-run`. mq never runs commands through a shell.

```bash
$ mq -I null --allow-run=df -F json 'import "cmd" | cmd::run("df", ["-k"]) | filter(fn(r): r["mounted_on"] == "/";)'
```

## Detect the command automatically

`cmd::parse_auto` recognizes the command from the output text (header line or line shape), so you do not have to name the parser:

```bash
$ echo 'uid=501(alice) gid=20(staff) groups=20(staff)' | mq -I raw -F json 'import "cmd" | cmd::parse_auto'
```

```json
{
  "uid": 501,
  "user": "alice",
  "gid": 20,
  "group": "staff",
  "groups": [
    {
      "id": 20,
      "name": "staff"
    }
  ]
}
```

It never guesses. Each parser has a signature that is matched against the first 20 lines of the text, and it describes the whole shape of the output, so text that mixes two formats is refused rather than read as one of them. If no parser fits, or several do, `parse_auto` raises an error that lists the candidates. Formats that are too generic to recognize (`wc`, `du`, `git status`, ...) are not claimed at all. In those cases name the parser with `cmd::parse_output("git status", .)`. `cmd::detect` returns the matching keys without parsing.

Naming a parser does not skip the check: `parse_output("id", .)` raises an error when the text does not look like `id` output. Call the parser function itself, such as `cmd::id_parse`, to bypass it.

## Notes

- The cmd module has parsers for more than 80 commands, among them `ps`, `df`, `ls -l`, `id`, `ping`, `lsof`, `ip addr`, `ss`, `git log`, `git status`, `pip list`, `npm ls` and `docker ps`. See the [cmd.mq README](https://github.com/harehare/cmd.mq) for the full list. `run` picks the parser from the command name, and commands with subcommands are keyed by the first non-flag argument (`git log`, `docker ps`, `ip addr`).
- Command parsers convert plainly numeric fields (`pid`, `size`, ...) to numbers. Everything else stays a string, and `df -h` sizes such as `460Gi` are not guessed.
- For commands without a dedicated parser, use a generic one. `columns_parse` handles whitespace-separated columns with a header line. `table_parse` handles columns aligned under the header, such as `docker ps` and `kubectl get`, where values contain spaces. `kv_parse` handles `key: value` text.
- Column names differ by platform. `df` reports `capacity` on macOS and `use_percent` on Linux, so check the keys with `-F json` before filtering.
- Going the other way, from Markdown tables to CSV? See [Convert a Markdown table to CSV](convert-table-to-csv.md).
