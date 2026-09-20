# Turn command output into structured data

Goal: Parse the text printed by CLI commands such as `ps`, `df`, `ls -l` and `git log` into arrays of dicts, then filter it or render it as a Markdown table or JSON.

Prerequisites: The [cmdparse.mq](https://github.com/harehare/cmdparse.mq) extension module. Copy `cmdparse.mq` into your module directory, or import it over HTTP with `--allow-http-import` and `import "github.com/harehare/cmdparse.mq"`. Pipe the command's output in and read it with `-I raw`, which gives the query the whole text as a single string.

## Query

The processes using the most memory, as a Markdown table:

```bash
$ ps aux | mq -I raw 'import "cmdparse" | import "csv" | cmdparse::ps_parse(.) | sort_by(fn(p): -p["mem_percent"];) | map(fn(p): {"pid": p["pid"], "mem_percent": p["mem_percent"], "command": p["command"]};) | slice(0, 2) | csv::csv_to_markdown_table()'
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
$ git log -2 | mq -I raw 'import "cmdparse" | cmdparse::git_log_parse(.) | map(fn(c): "- " + slice(c["commit"], 0, 7) + " " + first(split(c["message"], "\n")) + " (" + c["author"] + ")";) | join("\n")'
```

```markdown
- 9fceb02 fix(parser): handle empty input (Alice <alice@example.com>)
- 1a2b3c4 feat(cli): add --watch flag (Bob <bob@example.com>)
```

## Run the command from mq

`cmdparse::run(cmd, args)` runs the command and parses its output in one call, so no pipe or `-I raw` is needed. It uses the `system` function, which requires `--allow-run`. mq never runs commands through a shell.

```bash
$ mq -I null --allow-run=df -F json 'import "cmdparse" | cmdparse::run("df", ["-k"]) | filter(fn(r): r["mounted_on"] == "/";)'
```

## Notes

- The command parsers are `ps_parse`, `df_parse`, `du_parse`, `ls_parse`, `id_parse`, `wc_parse`, `ping_parse`, `lsof_parse`, `free_parse`, `pip_list_parse`, `git_log_parse`, `git_status_parse` and `git_branch_parse`. `run` picks the right one from the command name, and `git`, `docker`, `kubectl` and `pip` are keyed by their first argument (`git log`, `docker ps`).
- Command parsers convert plainly numeric fields (`pid`, `size`, ...) to numbers. Everything else stays a string, and `df -h` sizes such as `460Gi` are not guessed.
- For commands without a dedicated parser, use a generic one. `columns_parse` handles whitespace-separated columns with a header line. `table_parse` handles columns aligned under the header, such as `docker ps` and `kubectl get`, where values contain spaces. `kv_parse` handles `key: value` text.
- Column names differ by platform. `df` reports `capacity` on macOS and `use_percent` on Linux, so check the keys with `-F json` before filtering.
- Going the other way, from Markdown tables to CSV? See [Convert a Markdown table to CSV](convert-table-to-csv.md).
