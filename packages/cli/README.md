# @dhruv2mars/codexchat

Chat with ChatGPT from your terminal.

## Install

Public status on March 20, 2026:
- npm package is published
- GitHub release assets are published

Install with any supported package manager:

```bash
npm i -g @dhruv2mars/codexchat
```

```bash
bun install -g @dhruv2mars/codexchat
```

First run downloads the native `codexchat` binary and pinned `codex` bridge into `~/.codexchat/bin/`.
Release assets come from GitHub Releases for your platform.
Auth runs through the official `codex app-server` bridge and ChatGPT sign-in.
The app stores local config and thread history only.

For local dev, use the repo directly:

```bash
bun install
cargo run -p codexchat-cli --
```

Supported release binaries:
- `darwin-arm64`
- `darwin-x64`
- `linux-arm64`
- `linux-x64`
- `win32-arm64`
- `win32-x64`

## Usage

```bash
codexchat
```

One-shot prompt:

```bash
codexchat chat "indian capital city?"
```

Model list:

```bash
codexchat models
```

Manual upgrade:

```bash
codexchat update
```

The updater prefers the original install manager when possible.

## Auth

```bash
codexchat auth login
codexchat auth status
codexchat auth logout
```

## TUI keys

- `Enter` send
- `Shift+Enter` newline
- `Tab` / `Shift+Tab` move focus
- `Ctrl+K` models
- `Ctrl+J` threads
- `Ctrl+M` composer
- `Ctrl+N` new chat
- `Ctrl+L` logout
- `Esc` stop stream or clear search
- `q` quit

## Install behavior

- no postinstall is required
- first run bootstraps the native binary and pinned `codex` bridge
- this keeps Bun global installs usable even when script trust is locked down
