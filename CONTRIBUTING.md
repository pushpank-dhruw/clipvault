# Contributing

## Getting Started

```bash
git clone https://github.com/pushpank-dhruw/clipvault
cd clipvault
cargo build
```

## Code Style

- `cargo fmt` before committing
- `cargo clippy --all-targets --all-features --locked -- -D warnings` must pass
- Never use `unwrap()` or `expect()` outside tests
- One assertion per test, descriptive names (`should_*`)
- Follow `rustfmt` defaults

## Commit Messages

Conventional commit format: `type: description`

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation
- `style:` — formatting
- `refactor:` — code restructuring
- `test:` — tests
- `chore:` — build/config

## Pull Request Process

1. Branch from `main`
2. One logical change per commit
3. All tests must pass (`cargo test`)
4. Clippy must be clean
5. Update `ROADMAP.md` if completing a milestone item

## Architecture

See [`AGENTS.md`](AGENTS.md) for the full data flow and architecture overview.
