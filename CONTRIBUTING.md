# Contributing to chunkfetti

## Commit messages decide the version

Every push to `main` becomes a release. The version comes from the commits since the last release:

| Commit starts with | Release | Example |
|---|---|---|
| `feat:` | minor (0.**3**.0) | `feat: stream tiles by distance` |
| `fix:`, `perf:`, anything else | patch (0.2.**1**) | `fix: water faces drawn twice` |
| `feat!:` or a `BREAKING CHANGE:` line | minor while we are at 0.x, major after 1.0 | `feat!: new tile layout` |

The release notes are the list of those commit subjects, grouped by kind.

## Never commit game or mod files

No `.jar`, textures, models, sounds, worlds or region files, not even small ones for tests. Tests build their own tiny
worlds in code. Assets are read from the user's install at build time. `.gitignore` blocks the usual suspects; CI
fails if any slip through.

## Building

```bash
cargo test
```

```bash
cd viewer && npm install && npm run build
```
