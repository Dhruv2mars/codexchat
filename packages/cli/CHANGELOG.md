# @dhruv2mars/codexchat

## 0.1.8

### Patch Changes

- Show clear auth status text for signed-out users on fresh installs.

## 0.1.7

### Patch Changes

- Switch npm release publishing to GitHub trusted publishing.
- Remove the old token fallback path from the release workflow.
- Refresh public package docs to reflect live npm and GitHub installs.

## 0.1.6

### Patch Changes

- Fix installer package-version lookup so first-run native binary download uses the real published version instead of falling back to `v0.0.0`.
- Restore Bun global install download flow end to end.

## 0.1.5

### Patch Changes

- Fix Bun global install path resolution so the launcher follows the real package path instead of the symlink shim path.
- Restore first-run native binary download for Bun-installed `codexchat`.

## 0.1.4

### Patch Changes

- Remove all Keychain and system secret-store usage from auth storage paths.
- Save local auth only in `~/.codexchat/session.json` with locked file permissions.
- Remove the old bridge Keychain path too, so no shipped app path depends on system secret-store prompts.

## 0.1.3

### Patch Changes

- Publish the first working Rust CLI release with ChatGPT auth via official Codex bridge, model listing, terminal chat, and npm/GitHub Release distribution.
