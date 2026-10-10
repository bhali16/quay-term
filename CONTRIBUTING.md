# Contributing to quay-term

Thanks for your interest. The project is in early planning, so the best contributions right now are ideas, issue reports and design feedback.

## Before you start

- Read the [Code of Conduct](CODE_OF_CONDUCT.md).
- Read the [clean-room policy](docs/CLEAN_ROOM.md). Do not contribute code or assets derived from proprietary products.
- For anything larger than a small fix, open an issue first so we can agree on the approach.

## Development setup

Requirements (once the app skeleton lands):

- Rust (stable) via [rustup](https://rustup.rs)
- Node.js LTS
- Tauri 2 platform prerequisites: https://tauri.app/start/prerequisites/

Common commands (will be finalized in M0):

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Keep changes focused. One topic per PR.
3. Add or update tests where it makes sense.
4. Make sure `cargo fmt`, `clippy` and tests pass.
5. Update `CHANGELOG.md` for user-visible changes.
6. Fill in the PR template.

Commit messages: short imperative summary (for example `Add known_hosts verification`), with details in the body if needed.

## License

By contributing you agree that your contributions are licensed under GPL-3.0, the same as the project.
