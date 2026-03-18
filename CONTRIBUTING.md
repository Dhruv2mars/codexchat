# Contributing

## Rules

- tests first
- atomic commits only: `feat:`, `fix:`, `test:`, `docs:`
- branch -> PR -> merge -> delete branch
- verify real behavior before asking for review

## Local checks

```bash
bun run test
bun run check
bun run build
```

For CLI-only work, also run:

```bash
cargo test --workspace
cargo run -p codexchat-cli -- --help
bun run cli
```

## Real verification

Close the loop yourself:

1. run `codexchat`
2. log in with ChatGPT
3. load models
4. send a real prompt
5. verify thread resume and logout

No fake or mocked final verification.

## Release changes

If shipped behavior changes, add a changeset:

```bash
bunx changeset
```

Version packages:

```bash
bun run release:version
```

Publish from tag:

```bash
TAG="v$(node -p \"require('./packages/cli/package.json').version\")"
git tag "$TAG"
git push origin "$TAG"
```
