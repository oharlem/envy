# Contributing to varz

## Prerequisites

Install `cargo-nextest` if you don't have it:

```sh
cargo install cargo-nextest --locked
```

## Build

```sh
cargo build
```

## Required checks before submitting a PR

All of the following must pass with zero warnings:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
```

Run them together:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo nextest run
```

## Guidelines

- **One change per PR.** Keep scope small and focused.
- **No unrelated churn.** Only modify what the task requires.
- **Tests are mandatory.** New behavior must be covered by tests.
- **Zero warnings policy.** Clippy must pass with `-D warnings`.
- **Keep `README.md` in sync** when user-facing behavior changes.
- **Do not weaken security defaults** — `~/.varz_env` permissions (0600), secret masking, or the `--init` / `set` / `unset` protocol.

## Reporting issues

Open an issue with a minimal reproduction case. For security concerns, please report privately via the repository contact.
