# Changesets

Use changesets for every release-impacting change.

Add a changeset:
```bash
bunx changeset
```

Version packages:
```bash
bun run release:version
```

Commit version updates, then push release tag:
```bash
TAG="v$(node -p \"require('./packages/cli/package.json').version\")"
git tag "$TAG"
git push origin "$TAG"
```
