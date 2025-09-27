#!/usr/bin/env bash

set -e

# mq installation script
# Based on the Turso CLI installer
# Usage: curl -sSL https://raw.githubusercontent.com/harehare/mq/main/scripts/install.sh | bash

readonly MQ_REPO="harehare/mq"
readonly MQ_INSTALL_DIR="$HOME/.mq"
readonly MQ_BIN_DIR="$MQ_INSTALL_DIR/bin"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly PURPLE='\033[0;35m'
readonly CYAN='\033[0;36m'
readonly BOLD='\033[1m'
readonly NC='\033[0m' # No Color

# Utility functions
log() {
    echo -e "${GREEN}ℹ${NC}  $*"
}

warn() {
    echo -e "${YELLOW}⚠${NC}  $*"
}

error() {
    echo -e "${RED}✗${NC}  $*" >&2
    exit 1
}

# Display the mq logo
show_logo() {
    cat << 'EOF'

    ███╗   ███╗  ██████╗
    ████╗ ████║ ██╔═══██╗
    ██╔████╔██║ ██║   ██║
    ██║╚██╔╝██║ ██║▄▄ ██║
    ██║ ╚═╝ ██║ ╚██████╔╝
    ╚═╝     ╚═╝  ╚══▀▀═╝

EOF
    echo -e "${BOLD}${CYAN}     Markdown Query Language${NC}"
    echo -e "${BLUE}   mq is a command-line tool that processes${NC}"
    echo -e "${BLUE}     Markdown using a syntax similar to jq${NC}"
    echo ""
    echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Detect the operating system
detect_os() {
    case "$(uname -s)" in
        Linux*)
            echo "linux"
            ;;
        Darwin*)
            echo "darwin"
            ;;
        CYGWIN*|MINGW*|MSYS*)
            echo "windows"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            ;;
    esac
}

# Detect the architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)
            echo "x86_64"
            ;;
        aarch64|arm64)
            echo "aarch64"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            ;;
    esac
}

# Get the latest release version from GitHub
get_latest_version() {
    local version
    version=$(curl -s "https://api.github.com/repos/$MQ_REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [[ -z "$version" ]]; then
        error "Failed to get the latest version"
    fi

    echo "$version"
}

# Construct the download URL for the binary
get_download_url() {
    local version="$1"
    local os="$2"
    local arch="$3"
    local ext=""

    if [[ "$os" == "windows" ]]; then
        ext=".exe"
        target="${arch}-pc-windows-msvc"
    elif [[ "$os" == "darwin" ]]; then
        target="${arch}-apple-darwin"
    else
        target="${arch}-unknown-linux-gnu"
    fi

    echo "https://github.com/$MQ_REPO/releases/download/$version/mq-${target}${ext}"
}

# Download and install mq
install_mq() {
    local version="$1"
    local os="$2"
    local arch="$3"
    local download_url
    local binary_name="mq"
    local ext=""

    if [[ "$os" == "windows" ]]; then
        ext=".exe"
        binary_name="mq.exe"
    fi

    download_url=$(get_download_url "$version" "$os" "$arch")

    log "Downloading mq $version for $os/$arch..."
    log "Download URL: $download_url"

    # Create installation directory
    mkdir -p "$MQ_BIN_DIR"

    # Download the binary
    local temp_file
    temp_file=$(mktemp)

    if ! curl -L --progress-bar "$download_url" -o "$temp_file"; then
        error "Failed to download mq binary"
    fi

    # Move and make executable
    mv "$temp_file" "$MQ_BIN_DIR/$binary_name"
    chmod +x "$MQ_BIN_DIR/$binary_name"

    log "mq installed successfully to $MQ_BIN_DIR/$binary_name"
}

# Add mq to PATH by updating shell profile
update_shell_profile() {
    local shell_profile=""
    local shell_name
    shell_name=$(basename "$SHELL")

    case "$shell_name" in
        bash)
            if [[ -f "$HOME/.bashrc" ]]; then
                shell_profile="$HOME/.bashrc"
            elif [[ -f "$HOME/.bash_profile" ]]; then
                shell_profile="$HOME/.bash_profile"
            fi
            ;;
        zsh)
            if [[ -f "$HOME/.zshrc" ]]; then
                shell_profile="$HOME/.zshrc"
            fi
            ;;
        fish)
            if [[ -d "$HOME/.config/fish" ]]; then
                shell_profile="$HOME/.config/fish/config.fish"
                mkdir -p "$(dirname "$shell_profile")"
            fi
            ;;
    esac

    if [[ -n "$shell_profile" ]]; then
        local path_export
        if [[ "$shell_name" == "fish" ]]; then
            path_export="set -gx PATH \$PATH $MQ_BIN_DIR"
        else
            path_export="export PATH=\"\$PATH:$MQ_BIN_DIR\""
        fi

        if ! grep -q "$MQ_BIN_DIR" "$shell_profile" 2>/dev/null; then
            echo "" >> "$shell_profile"
            echo "# Added by mq installer" >> "$shell_profile"
            echo "$path_export" >> "$shell_profile"
            log "Added $MQ_BIN_DIR to PATH in $shell_profile"
        else
            warn "$MQ_BIN_DIR already exists in $shell_profile"
        fi
    else
        warn "Could not detect shell profile to update"
        warn "Please manually add $MQ_BIN_DIR to your PATH"
    fi
}

# Verify installation
verify_installation() {
    if [[ -x "$MQ_BIN_DIR/mq" ]] || [[ -x "$MQ_BIN_DIR/mq.exe" ]]; then
        log "Installation verified successfully!"
        return 0
    else
        error "Installation verification failed"
    fi
}

# Show post-installation instructions
show_post_install() {
    echo ""
    echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}${GREEN}✨ mq has been installed successfully! ✨${NC}"
    echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo -e "${BOLD}${CYAN}🚀 Getting Started:${NC}"
    echo ""
    echo -e "  ${YELLOW}1.${NC} Restart your terminal or run:"
    echo -e "     ${CYAN}source ~/.bashrc${NC} ${BLUE}(or your shell's profile)${NC}"
    echo ""
    echo -e "  ${YELLOW}2.${NC} Verify the installation:"
    echo -e "     ${CYAN}mq --version${NC}"
    echo ""
    echo -e "  ${YELLOW}3.${NC} Get help:"
    echo -e "     ${CYAN}mq --help${NC}"
    echo ""
    echo -e "${BOLD}${CYAN}⚡ Quick Examples:${NC}"
    echo -e "  ${GREEN}▶${NC} ${CYAN}echo '# Hello World' | mq '.h'${NC}"
    echo -e "  ${GREEN}▶${NC} ${CYAN}mq 'select(type == \"heading\")' README.md${NC}"
    echo ""
    echo -e "${BOLD}${CYAN}📚 Learn More:${NC}"
    echo -e "  ${GREEN}▶${NC} Documentation: ${BLUE}https://mqlang.org/${NC}"
    echo -e "  ${GREEN}▶${NC} Repository:    ${BLUE}https://github.com/$MQ_REPO${NC}"
    echo ""
    echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Main installation function
main() {
    show_logo

    # Check if curl is available
    if ! command -v curl &> /dev/null; then
        error "curl is required but not installed"
    fi

    # Detect system
    local os arch version
    os=$(detect_os)
    arch=$(detect_arch)

    log "Detected system: $os/$arch"

    # Get latest version
    version=$(get_latest_version)
    log "Latest version: $version"

    # Install mq
    install_mq "$version" "$os" "$arch"

    # Update shell profile
    update_shell_profile

    # Verify installation
    verify_installation

    # Show post-installation instructions
    show_post_install
}

# Handle script arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            echo "mq installation script"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --help, -h    Show this help message"
            echo "  --version, -v Show version and exit"
            echo ""
            echo "Environment variables:"
            echo "  MQ_VERSION    Install specific version (default: latest)"
            echo ""
            echo "Examples:"
            echo "  $0                    # Install latest version"
            echo "  MQ_VERSION=v0.2.23 $0  # Install specific version"
            exit 0
            ;;
        --version|-v)
            echo "mq installer v1.0.0"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
    shift
done

# Check if we're running in a supported environment
if [[ -z "${BASH_VERSION:-}" ]]; then
    error "This installer requires bash"
fi

# Run the main installation
main "$@"
