# Contributing to TaskHub

> **Read this before opening a PR.**

TaskHub is solo-developed until M7 (public launch, ~month 10). External contributions are welcome but scope is intentionally narrow until the core is stable.

## Before contributing

1. Check the [project charter](PROJECT.md) — anti-goals section especially.
2. Open an issue first for anything beyond a trivial fix.
3. No feature contributions until M6 (dogfood + beta). Bug fixes always welcome.

## Setup

Requirements: Rust stable (latest), `cargo`.

```sh
git clone https://github.com/pietroairoldi/taskhub
cd taskhub
cargo build
cargo test
```

## Code style

```sh
cargo fmt          # must pass
cargo clippy -- -D warnings  # must pass
cargo test --all   # must pass
```

CI enforces all three. No exceptions.

## Commit style

Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.

## Plugin contributions

Plugin contributions welcome from M3 onward. Use `taskhub plugin new <name>` to scaffold.

## What not to contribute

See PROJECT.md §5 (Anti-goals). PRs adding GUI, cloud features, multi-user, or anything outside the milestone scope will be closed without merge.
