//! `--dump-bytecode` output formats.
use clap::ValueEnum;
use colored::Colorize;
use mq_lang::BytecodeDump;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(super) enum BytecodeFormat {
    /// One instruction per line
    #[default]
    Text,
    /// Phases, chunks, instructions, and constants as JSON
    Json,
    /// A table of instructions per chunk
    Markdown,
}

impl BytecodeFormat {
    /// Only the text format is colored.
    pub(super) fn render(self, dump: &BytecodeDump, color: bool) -> String {
        match self {
            Self::Text => render_text(dump, color),
            Self::Json => render_json(dump),
            Self::Markdown => render_markdown(dump),
        }
    }
}

struct Palette {
    color: bool,
}

impl Palette {
    fn title(&self, text: &str) -> String {
        if self.color {
            text.bright_cyan().bold().to_string()
        } else {
            text.to_string()
        }
    }

    fn chunk(&self, text: &str) -> String {
        if self.color {
            text.bright_yellow().bold().to_string()
        } else {
            text.to_string()
        }
    }

    fn section(&self, text: &str) -> String {
        if self.color {
            text.bright_green().bold().to_string()
        } else {
            text.to_string()
        }
    }

    fn field(&self, label: &str, value: &str) -> String {
        if self.color {
            format!("{}: {}", label.cyan(), value.dimmed())
        } else {
            format!("{label}: {value}")
        }
    }

    fn pc(&self, text: &str) -> String {
        if self.color {
            text.dimmed().to_string()
        } else {
            text.to_string()
        }
    }

    fn opcode(&self, text: &str) -> String {
        if self.color {
            text.bright_blue().to_string()
        } else {
            text.to_string()
        }
    }
}

fn slot_list(names: &[String]) -> String {
    names
        .iter()
        .enumerate()
        .map(|(slot, name)| format!("{slot}:{name}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_text(dump: &BytecodeDump, color: bool) -> String {
    let palette = Palette { color };
    let mut output = String::new();
    for (phase_index, phase) in dump.phases.iter().enumerate() {
        if phase_index > 0 {
            output.push('\n');
        }
        let _ = writeln!(output, "{}", palette.title("Tarn VM bytecode"));
        let _ = writeln!(output, "  {}", palette.field("phase", &phase.name));
        let _ = writeln!(output, "  {}", palette.field("chunks", &phase.chunks.len().to_string()));
        for chunk in &phase.chunks {
            let _ = writeln!(output, "\n{}", palette.chunk(&format!("Chunk {}", chunk.index)));
            let _ = writeln!(output, "  {}", palette.section("frame"));
            let _ = writeln!(
                output,
                "    {}",
                palette.field(
                    "local slots",
                    &format!("{} ({})", chunk.local_count, slot_list(&chunk.locals))
                )
            );
            let _ = writeln!(
                output,
                "    {}",
                palette.field(
                    "upvalues",
                    &format!("{} ({})", chunk.upvalues.len(), slot_list(&chunk.upvalues))
                )
            );
            let _ = writeln!(output, "  {}", palette.section("instructions"));
            for instruction in &chunk.instructions {
                let opcode = if instruction.operands.is_empty() {
                    instruction.opcode.clone()
                } else {
                    format!("{} {}", instruction.opcode, instruction.operands)
                };
                let location = instruction
                    .location
                    .map(|location| {
                        let location = format!("{}:{}", location.line, location.column);
                        format!(" @ {}", if color { location.dimmed().to_string() } else { location })
                    })
                    .unwrap_or_default();
                let _ = writeln!(
                    output,
                    "    {}  {}{location}",
                    palette.pc(&format!("{:04}", instruction.pc)),
                    palette.opcode(&opcode)
                );
            }
            if !chunk.constants.is_empty() {
                let _ = writeln!(output, "  {}", palette.section("constants"));
                for (index, value) in chunk.constants.iter().enumerate() {
                    let _ = writeln!(output, "    [{index}] {value}");
                }
            }
        }
    }
    output
}

fn render_json(dump: &BytecodeDump) -> String {
    let mut rendered = serde_json::to_string_pretty(dump)
        .unwrap_or_else(|error| serde_json::json!({ "error": error.to_string() }).to_string());
    rendered.push('\n');
    rendered
}

/// An inline code span that is safe in a table cell.
fn markdown_code(text: &str) -> String {
    let text = text.replace('\r', "").replace('\n', "\\n");
    if text.is_empty() {
        return "` `".to_string();
    }
    let longest_run = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run + 1);
    let padding = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{padding}{text}{padding}{fence}").replace('|', "\\|")
}

fn markdown_slots(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_string();
    }
    names
        .iter()
        .enumerate()
        .map(|(slot, name)| markdown_code(&format!("{slot}:{name}")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_markdown(dump: &BytecodeDump) -> String {
    let mut output = String::from("# Tarn VM bytecode\n");
    for phase in &dump.phases {
        let _ = writeln!(output, "\n## Phase: {}\n\nChunks: {}", phase.name, phase.chunks.len());
        for chunk in &phase.chunks {
            let _ = writeln!(output, "\n### Chunk {}\n", chunk.index);
            let _ = writeln!(
                output,
                "- Local slots ({}): {}",
                chunk.local_count,
                markdown_slots(&chunk.locals)
            );
            let _ = writeln!(
                output,
                "- Upvalues ({}): {}",
                chunk.upvalues.len(),
                markdown_slots(&chunk.upvalues)
            );
            output.push_str("\n| PC | Opcode | Location |\n| ---: | --- | --- |\n");
            for instruction in &chunk.instructions {
                let opcode = if instruction.operands.is_empty() {
                    instruction.opcode.clone()
                } else {
                    format!("{} {}", instruction.opcode, instruction.operands)
                };
                let location = instruction
                    .location
                    .map(|location| format!("{}:{}", location.line, location.column))
                    .unwrap_or_default();
                let _ = writeln!(
                    output,
                    "| {:04} | {} | {location} |",
                    instruction.pc,
                    markdown_code(&opcode)
                );
            }
            if !chunk.constants.is_empty() {
                output.push_str("\n| Constant | Value |\n| ---: | --- |\n");
                for (index, value) in chunk.constants.iter().enumerate() {
                    let _ = writeln!(output, "| {index} | {} |", markdown_code(value));
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::{BytecodeChunk, BytecodeInstruction, BytecodeLocation, BytecodePhase};
    use rstest::rstest;

    fn instruction(pc: usize, opcode: &str, operands: &str, location: Option<(usize, usize)>) -> BytecodeInstruction {
        BytecodeInstruction {
            pc,
            opcode: opcode.to_string(),
            operands: operands.to_string(),
            location: location.map(|(line, column)| BytecodeLocation { line, column }),
        }
    }

    fn sample() -> BytecodeDump {
        BytecodeDump {
            phases: vec![BytecodePhase {
                name: "main".to_string(),
                chunks: vec![BytecodeChunk {
                    index: 0,
                    local_count: 2,
                    locals: vec!["self".to_string(), "x".to_string()],
                    upvalues: Vec::new(),
                    instructions: vec![
                        instruction(0, "Const", "0", Some((1, 5))),
                        instruction(1, "CallBuiltin", "add, argc=1", Some((1, 1))),
                        instruction(2, "Return", "", None),
                    ],
                    constants: vec!["a|b".to_string(), "line1\nline2".to_string()],
                }],
            }],
        }
    }

    #[test]
    fn test_text_format() {
        let expected = "\
Tarn VM bytecode
  phase: main
  chunks: 1

Chunk 0
  frame
    local slots: 2 (0:self, 1:x)
    upvalues: 0 ()
  instructions
    0000  Const 0 @ 1:5
    0001  CallBuiltin add, argc=1 @ 1:1
    0002  Return
  constants
    [0] a|b
    [1] line1
line2
";
        assert_eq!(BytecodeFormat::Text.render(&sample(), false), expected);
    }

    #[test]
    fn test_text_format_separates_phases() {
        let mut dump = sample();
        dump.phases.push(BytecodePhase {
            name: "nodes aggregate".to_string(),
            chunks: Vec::new(),
        });
        let text = BytecodeFormat::Text.render(&dump, false);
        assert!(text.contains("\n\nTarn VM bytecode\n  phase: nodes aggregate\n  chunks: 0\n"));
    }

    #[test]
    fn test_json_format() {
        let rendered = BytecodeFormat::Json.render(&sample(), true);
        assert!(rendered.ends_with('\n'));
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        let chunk = &value["phases"][0]["chunks"][0];
        assert_eq!(value["phases"][0]["name"], "main");
        assert_eq!(chunk["locals"], serde_json::json!(["self", "x"]));
        assert_eq!(
            chunk["instructions"][1],
            serde_json::json!({
                "pc": 1,
                "opcode": "CallBuiltin",
                "operands": "add, argc=1",
                "location": {"line": 1, "column": 1},
            })
        );
        assert_eq!(
            chunk["instructions"][2],
            serde_json::json!({"pc": 2, "opcode": "Return", "location": null})
        );
        assert_eq!(chunk["constants"][1], "line1\nline2");
    }

    #[test]
    fn test_markdown_format() {
        let expected = "\
# Tarn VM bytecode

## Phase: main

Chunks: 1

### Chunk 0

- Local slots (2): `0:self`, `1:x`
- Upvalues (0): none

| PC | Opcode | Location |
| ---: | --- | --- |
| 0000 | `Const 0` | 1:5 |
| 0001 | `CallBuiltin add, argc=1` | 1:1 |
| 0002 | `Return` |  |

| Constant | Value |
| ---: | --- |
| 0 | `a\\|b` |
| 1 | `line1\\nline2` |
";
        assert_eq!(BytecodeFormat::Markdown.render(&sample(), true), expected);
    }

    #[rstest]
    #[case::plain("Const 0", "`Const 0`")]
    #[case::empty("", "` `")]
    #[case::pipe("a|b", "`a\\|b`")]
    #[case::newline("a\nb", "`a\\nb`")]
    #[case::backtick("a`b", "``a`b``")]
    #[case::edge_backtick("`a`", "`` `a` ``")]
    fn test_markdown_code(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(markdown_code(text), expected);
    }
}
