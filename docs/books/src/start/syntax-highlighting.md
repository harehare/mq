# Syntax Highlighting

## Using bat

[bat](https://github.com/sharkdp/bat) is a `cat` clone with syntax highlighting and Git integration. You can use mq's Sublime syntax file to enable syntax highlighting for mq files in bat.

### Setting up mq syntax highlighting

1. Create the bat syntax directory if it doesn't exist:

```sh
mkdir -p "$(bat --config-dir)/syntaxes"
```

2. Copy the mq syntax file:

```sh
# Clone the mq repository or download mq.sublime-syntax
curl -o "$(bat --config-dir)/syntaxes/mq.sublime-syntax" \
  https://raw.githubusercontent.com/harehare/mq/main/assets/mq.sublime-syntax
```

3. Rebuild bat's cache:

```sh
bat cache --build
```

### Usage

Now you can use bat to display mq files with syntax highlighting:

```sh
# View an mq file with syntax highlighting
bat query.mq
```

### Example

Create a sample mq file:

```sh
cat > example.mq << 'EOF'
# This is a comment
def greet(name):
  s"Hello, ${name}!"
end

.h | .text | greet("World")
EOF
```

View it with syntax highlighting:

```sh
bat example.mq
```

## Helix

[Helix](https://helix-editor.com/) discovers languages, tree-sitter grammars, and LSP servers through its `languages.toml` config file rather than a dedicated plugin. mq syntax highlighting is provided by [tree-sitter-mq](https://github.com/harehare/tree-sitter-mq), and language features (completion, hover, diagnostics) by [`mq-lsp`](install.md#mq-lsp-language-server).

### Setting up mq support

1. Add the following to `languages.toml` in your Helix config directory (`~/.config/helix/languages.toml`):

```toml
[[language]]
name = "mq"
scope = "source.mq"
file-types = ["mq"]
comment-token = "#"
language-servers = ["mq-lsp"]
indent = { tab-width = 2, unit = "  " }

[language-server.mq-lsp]
command = "mq-lsp"

[[grammar]]
name = "mq"
source = { git = "https://github.com/harehare/tree-sitter-mq", rev = "main" }
```

2. Fetch and build the grammar:

```sh
hx --grammar fetch
hx --grammar build
```

3. Copy the highlight queries from tree-sitter-mq into your Helix runtime directory, since Helix does not bundle queries for externally configured grammars:

```sh
mkdir -p ~/.config/helix/runtime/queries/mq
curl -o ~/.config/helix/runtime/queries/mq/highlights.scm \
  https://raw.githubusercontent.com/harehare/tree-sitter-mq/main/queries/highlights.scm
```

4. Make sure [`mq-lsp`](install.md#mq-lsp-language-server) is installed and on your `PATH`.

5. Verify the setup:

```sh
hx --health mq
```

This should report the tree-sitter grammar as found and `mq-lsp` as configured.

## Editor Support

In addition to bat, mq syntax highlighting is available for:

- **Visual Studio Code**: Install the [mq extension](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
- **Obsidian**: Install the [mq plugin](https://community.obsidian.md/plugins/mq)
- **Helix**: See the [Helix](#helix) section above for `languages.toml` setup
