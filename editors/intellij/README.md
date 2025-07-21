# mq IntelliJ Plugin

IntelliJ IDEA plugin for the mq language - a jq-like tool for Markdown processing.

## Features

- **Syntax Highlighting**: Full syntax highlighting for `.mq` files
- **Language Support**: Code completion, validation, and error highlighting
- **LSP Integration**: Uses the mq Language Server for advanced features
- **Execute Queries**: Run mq queries directly from the IDE
- **File Templates**: Create new mq files with examples
- **Tool Window**: Dedicated output window for query results

## Installation

### From JetBrains Marketplace (Coming Soon)

1. Open IntelliJ IDEA
2. Go to File → Settings → Plugins
3. Search for "mq" in the marketplace
4. Install and restart IntelliJ IDEA

### Manual Installation

1. Download the plugin ZIP file from releases
2. Go to File → Settings → Plugins
3. Click the gear icon → Install Plugin from Disk
4. Select the downloaded ZIP file
5. Restart IntelliJ IDEA

## Requirements

- IntelliJ IDEA 2023.2 or later
- mq CLI tool installed (will be installed automatically if not present)

## Usage

### Creating mq Files

- **File → New → mq File**: Creates a new empty mq file
- **File → New → mq File with Examples**: Creates a new mq file with example queries

### Running Queries

- **Ctrl+Alt+M**: Run selected mq query on a chosen Markdown file
- **Ctrl+Alt+Q**: Execute mq query on current file
- **Ctrl+Alt+F**: Execute mq file on current document

### Configuration

Go to File → Settings → Tools → mq to configure:

- **mq executable path**: Custom path to mq binary
- **Show examples in new file**: Include example queries in new files
- **Enable LSP**: Enable Language Server Protocol support

## Development

### Building

```bash
./gradlew build
```

### Running in Development

```bash
./gradlew runIde
```

### Building Distribution

```bash
./gradlew buildPlugin
```

## Supported File Types

The plugin recognizes and can process:

- Markdown files (`.md`)
- MDX files (`.mdx`)  
- HTML files (`.html`)
- Text files (`.txt`)
- CSV files (`.csv`)
- TSV files (`.tsv`)

## Language Features

- Syntax highlighting for mq keywords, operators, and functions
- Code completion for built-in functions and common selectors
- Error highlighting and validation through LSP
- File structure navigation
- Quick documentation

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## License

This plugin is licensed under the same terms as the mq project (MIT License).