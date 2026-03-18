# codexchat

Chat with ChatGPT from your terminal.

`codexchat` is a local-first Rust TUI chat app. It launches the official `codex app-server` bridge over stdio, signs in with ChatGPT, filters to GPT-family models, and keeps only local config and thread history under `~/.codexchat/`.
It does not store ChatGPT tokens in app files.

## Why

- browser-to-localhost permissions were too brittle for the main product path
- official local bridge keeps auth out of app storage
- install is simple
- chat feels fast and direct

## Install

Use any supported package manager:

```bash
npm i -g @dhruv2mars/codexchat
```

```bash
bun install -g @dhruv2mars/codexchat
```

```bash
pnpm add -g @dhruv2mars/codexchat
```

First run downloads the native binary into `~/.codexchat/bin/`.
First run downloads the native binary and pinned codex bridge into `~/.codexchat/bin/`.

## Quickstart

```bash
codexchat
```

First run flow:
1. open `codexchat`
2. start ChatGPT sign-in
3. finish login in your browser
4. search/select a model
5. send a prompt
6. resume the thread later

## Commands

```bash
codexchat
codexchat auth login
codexchat auth status
codexchat auth logout
codexchat models
codexchat chat "indian capital city?"
codexchat update
```

## TUI keys

- `Enter` send
- `Shift+Enter` newline
- `Tab` / `Shift+Tab` move focus
- `Ctrl+K` focus models
- `Ctrl+J` focus threads
- `Ctrl+M` focus composer
- `Ctrl+N` new chat
- `Ctrl+L` logout
- `Esc` stop stream or clear search
- `q` quit

## Install notes

- `npm`, `bun`, `pnpm`, and `yarn` installs all use the same JS launcher
- the launcher downloads the right native binary and pinned `codex` bridge on first run
- no postinstall is required, so Bun global installs are usable without trusting scripts
- `codexchat update` upgrades with the same package manager used for install when possible
- the app talks only to the official `codex app-server` local bridge over stdio
- auth uses ChatGPT sign-in through Codex

## Release binaries

GitHub Releases publish:
- `darwin-arm64`
- `darwin-x64`
- `linux-arm64`
- `linux-x64`
- `win32-arm64`
- `win32-x64`

## Local dev

```bash
bun install
bun run cli
bun run test
bun run check
bun run build
```

Real manual verify:

```bash
cargo run -p codexchat-cli -- auth login
cargo run -p codexchat-cli -- models
cargo run -p codexchat-cli -- chat "indian capital city?"
cargo run -p codexchat-cli --
```

## Docs

- [Contributing](./CONTRIBUTING.md)
- [Security](./SECURITY.md)
- [Code of Conduct](./CODE_OF_CONDUCT.md)
- [CLI package docs](./packages/cli/README.md)

## Release

```bash
bunx changeset
bun run release:version
TAG="v$(node -p \"require('./packages/cli/package.json').version\")"
git tag "$TAG"
git push origin "$TAG"
```

Tags trigger:
- GitHub Release creation
- cross-platform binary builds
- npm publish
