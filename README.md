# varz

[![CI](https://github.com/oharlem/varz/actions/workflows/ci.yml/badge.svg)](https://github.com/oharlem/varz/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/varz.svg)](https://crates.io/crates/varz)

`export FOO=bar` works right now but vanishes when you close the terminal. Adding it to `~/.zshrc` manually works but is tedious and error-prone.

`varz set FOO bar` does both in one command — applies the change immediately to your current shell session and persists it automatically across future sessions.

It also gives you a searchable, colored overview of all your environment variables with automatic masking of secrets like API keys and tokens.

## Features

- Set and unset variables in the current shell session and persist them automatically
- List all environment variables sorted alphabetically
- Search variables by name with case-insensitive matching and highlighted results
- Mask values for keys that appear to contain secrets (API keys, tokens, passwords)
- File-based shell integration with no `eval` of binary output at runtime

## Installation

Requires current stable Rust (2024 edition, rustc 1.94+).

**From source:**
```sh
git clone https://github.com/oharlem/varz
cd varz
cargo install --path .
```

**Directly from GitHub:**
```sh
cargo install --git https://github.com/oharlem/varz
```

Both methods install to `~/.cargo/bin`. If needed, add it to your `PATH`:
```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

## Shell setup

**bash / zsh** — add to `~/.zshrc` or `~/.bashrc`:
```sh
eval "$(varz --init)"
```

**fish** — add to `~/.config/fish/config.fish`:
```fish
varz --init fish | source
```

Then reload your shell config (e.g. `source ~/.zshrc` or open a new terminal).

This installs a shell function that wraps the `varz` binary. When you run `varz set` or `varz unset`, the binary writes directives to a session-scoped mailbox file, and the shell function reads and applies them — no `eval` of binary output at runtime. The mailbox is ephemeral and cleaned up when the session ends. Only `~/.varz_env` persists across sessions.

Without this setup, `varz set` and `varz unset` still persist changes to `~/.varz_env` but cannot modify the current shell session. A manual fallback command is printed instead.


## Usage

```text
varz                     List all environment variables
varz <PATTERN>           Search environment variables by name
varz set KEY VALUE       Set in the current shell and persist to ~/.varz_env
varz unset KEY           Unset in the current shell and remove from ~/.varz_env
varz --init [SHELL]      Print shell integration code (bash, zsh, fish)
varz --version           Show version
varz --help              Show help
```

## Examples

```sh
# List and search
varz
varz OPEN
varz AWS

# Set and verify
varz set OPENAI_API_KEY 'sk-proj-abc123'
varz set GREETING 'hello world'
varz OPEN

# Remove
varz unset OPENAI_API_KEY
```

## Security

`~/.varz_env` is created with mode `0600` (owner read/write only), consistent with `~/.aws/credentials`, `~/.npmrc`, and `~/.netrc`. If the file has looser permissions, `varz` prints a warning on every invocation.

Values are stored in plain text — the same model as the tools listed above. The file lives at `~/.varz_env` rather than `~/.config/varz/` to reduce the chance of it being accidentally committed in a dotfiles repo. A future release may add optional OS keychain integration for stronger protection against accidental file exposure.

Displayed values are automatically masked for keys containing: `key`, `secret`, `token`, `password`, `passwd`, `pwd`, or `auth`.

## Notes

- Quote values containing spaces or special shell characters: `varz set GREETING 'hello world'`
- If the same key is exported elsewhere in your rc file, normal shell ordering applies — a later `export` wins, including over values from `~/.varz_env`
- After upgrading `varz`, re-source your shell rc or open a new terminal to pick up the latest init output

## License

Licensed under the [MIT license](LICENSE).
