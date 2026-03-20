# Agent notes — `varz`

## Scope
- Only change what the task requires. Avoid unrelated churn.

## Standards
- Keep `Cargo.toml` lean. Minimize dependencies.
- Treat `README.md` as the source of truth for user-facing behavior, install steps, and shell integration.
- Do not change persistence or safety defaults (`~/.varz_env` permissions, masking of secret-like keys, `varz --init`, `set`, `unset`) without updating tests and README.

## Validation
- Before submitting changes, run:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo nextest run`

## Compatibility
- Keep `rust-version` intentional. Do not raise it unless required.
- Preserve CLI behavior and shell integration unless the task explicitly requires a change.