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

## Editor Support

In addition to bat, mq syntax highlighting is available for:

- **Visual Studio Code**: Install the [mq extension](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
