use std::env;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// ANSI color codes
const BOLD: &str = "\x1b[1m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Visible prefix/suffix length when masking secret-like values
const MASK_EDGE_CHARS: usize = 4;
/// Standard env vars that contain secret keywords but are not secrets.
const NEVER_SECRET_KEYS: &[&str] = &["PWD", "OLDPWD"];

fn main() {
    #[cfg(unix)]
    warn_if_loose_permissions();

    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        // varz set KEY VALUE
        Some("set") => {
            let key = args.get(2);
            let value = args.get(3);
            match (key, value) {
                (Some(k), Some(v)) => cmd_set(k, v),
                _ => {
                    eprintln!("{}Usage: varz set KEY VALUE{}", BOLD, RESET);
                    std::process::exit(1);
                }
            }
        }

        // varz unset KEY
        Some("unset") => {
            let key = args.get(2);
            match key {
                Some(k) => cmd_unset(k),
                None => {
                    eprintln!("{}Usage: varz unset KEY{}", BOLD, RESET);
                    std::process::exit(1);
                }
            }
        }

        // varz --init [SHELL]  (prints shell function to eval)
        Some("--init") => {
            let shell = args.get(2).map(|s| s.as_str());
            match shell {
                Some("fish") => cmd_init_fish(),
                None | Some("bash") | Some("zsh") => cmd_init_posix(),
                Some(other) => {
                    eprintln!(
                        "{}Unknown shell '{}'. Supported: bash, zsh, fish{}",
                        BOLD, other, RESET
                    );
                    std::process::exit(1);
                }
            }
        }

        // varz --version
        Some("--version") | Some("-V") => {
            println!("varz {}", VERSION);
        }

        // varz --help
        Some("--help") | Some("-h") => print_help(),

        // varz PATTERN  — search env vars
        Some(pattern) => cmd_search(pattern),

        // varz  — list all
        None => cmd_list_all(),
    }
}

/// List all env vars sorted alphabetically
fn cmd_list_all() {
    let mut vars: Vec<(String, String)> = env::vars().collect();
    vars.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, value) in &vars {
        let display_value = mask_if_secret(key, value);
        println!(
            "{}{}{}={}{}{}{}",
            BOLD, key, RESET, DIM, GREEN, display_value, RESET
        );
    }
    println!("\n{}({} variables){}", DIM, vars.len(), RESET);
}

/// Search env vars (case-insensitive) and highlight matches
fn cmd_search(pattern: &str) {
    let pattern_lower = pattern.to_lowercase();
    let mut vars: Vec<(String, String)> = env::vars()
        .filter(|(k, _)| k.to_lowercase().contains(&pattern_lower))
        .collect();
    vars.sort_by(|a, b| a.0.cmp(&b.0));

    if vars.is_empty() {
        println!("{}No env vars matching '{}'{}", DIM, pattern, RESET);
        return;
    }

    for (key, value) in &vars {
        let highlighted_key = highlight(key, pattern);
        let display_value = mask_if_secret(key, value);
        println!(
            "{}{}={}{}{}",
            highlighted_key, RESET, DIM, display_value, RESET
        );
    }

    if vars.len() > 1 {
        println!("\n{}({} matches){}", DIM, vars.len(), RESET);
    }
}

/// Set an env var via the directive mailbox and persist it.
fn cmd_set(key: &str, value: &str) {
    // Validate key
    if !is_valid_key(key) {
        eprintln!("{}Invalid env var name: '{}'{}", BOLD, key, RESET);
        std::process::exit(1);
    }

    // Set the env var in the current session
    if write_directive(&format!("SET {key} {value}")) {
        println!("{}Set {}{}", GREEN, key, RESET);
    } else {
        // No mailbox — shell integration not loaded. Print a manual fallback.
        eprintln!(
            "{}warning:{} shell integration not loaded; run: export {}={}",
            YELLOW,
            RESET,
            key,
            shell_quote(value),
        );
    }

    // Persist to ~/.varz_env
    persist_set(key, value);
}

/// Unset an env var via the directive mailbox and remove it from persistence.
fn cmd_unset(key: &str) {
    if write_directive(&format!("UNSET {key}")) {
        println!("{}Unset {}{}", GREEN, key, RESET);
    } else {
        // No mailbox — shell integration not loaded. Print a manual fallback.
        eprintln!(
            "{}warning:{} shell integration not loaded; run: unset {}",
            YELLOW, RESET, key,
        );
    }

    persist_unset(key);
}

/// Returns the POSIX (bash/zsh) shell integration script.
fn posix_init_script() -> &'static str {
    r#"
# varz shell integration — directive mailbox protocol
# Add this to your ~/.zshrc or ~/.bashrc:
#   eval "$(varz --init)"

__varz_mailbox="${TMPDIR:-/tmp}/varz.$$"
mkdir -p -m 700 "$__varz_mailbox"
trap 'rm -rf "$__varz_mailbox"' EXIT

varz() {
  VARZ_MAILBOX="$__varz_mailbox/pending" command varz "$@"
  __varz_apply
}

__varz_apply() {
  local mbox="$__varz_mailbox/pending"
  [ -f "$mbox" ] || return 0

  while IFS=' ' read -r verb rest; do
    case "$verb" in
      SET)
        case "$rest" in
          *' '*)
            local key="${rest%% *}"
            local val="${rest#* }"
            ;;
          *)
            local key="$rest"
            local val=""
            ;;
        esac
        export "$key=$val"
        ;;
      UNSET)
        unset "$rest"
        ;;
      \#*|v[0-9]*|nonce:*) ;;
      *)
        echo "varz: unknown directive '$verb'" >&2
        ;;
    esac
  done < "$mbox"

  rm -f "$mbox"
}

# Load persisted varz env (no subprocess needed)
[ -f "$HOME/.varz_env" ] && . "$HOME/.varz_env"
"#
}

/// Print POSIX (bash/zsh) shell init function.
fn cmd_init_posix() {
    print!("{}", posix_init_script());
}

/// Returns the fish shell integration script.
fn fish_init_script() -> &'static str {
    r#"
# varz shell integration — directive mailbox protocol
# Add this to your ~/.config/fish/config.fish:
#   varz --init fish | source

set -l _varz_tmpdir
if set -q TMPDIR
    set _varz_tmpdir $TMPDIR
else
    set _varz_tmpdir /tmp
end
set -g __varz_mailbox "$_varz_tmpdir/varz.$fish_pid"
mkdir -p -m 700 $__varz_mailbox

function __varz_cleanup --on-event fish_exit
    rm -rf $__varz_mailbox
end

function varz
    set -lx VARZ_MAILBOX "$__varz_mailbox/pending"
    command varz $argv
    __varz_apply
end

function __varz_apply
    set -l mbox "$__varz_mailbox/pending"
    test -f $mbox; or return 0

    while read -l line
        set -l parts (string split -m 1 ' ' -- $line)
        switch $parts[1]
            case SET
                if test (count $parts) -ge 2
                    set -l kv (string split -m 1 ' ' -- $parts[2])
                    set -l key $kv[1]
                    set -l val ''
                    if test (count $kv) -ge 2
                        set val $kv[2]
                    end
                    set -gx $key $val
                end
            case UNSET
                if test (count $parts) -ge 2
                    set -e $parts[2]
                end
            case '#*' 'v*' 'nonce:*'
                # skip version headers and comments
            case '*'
                echo "varz: unknown directive '$parts[1]'" >&2
        end
    end < $mbox

    rm -f $mbox
end

# Load persisted varz env
if test -f $HOME/.varz_env
    while read -l line
        # Skip blank lines and comments
        string match -qr '^\s*$' -- $line; and continue
        string match -q '#*' -- $line; and continue
        # Strip "export " prefix, then split KEY=VALUE
        set -l stripped (string replace 'export ' '' -- $line)
        set -l kv (string split -m 1 '=' -- $stripped)
        if test (count $kv) -ge 2
            set -gx $kv[1] (string trim --chars="'" -- $kv[2])
        end
    end < $HOME/.varz_env
end
"#
}

/// Print fish shell init function.
fn cmd_init_fish() {
    print!("{}", fish_init_script());
}

fn print_help() {
    println!(
        r#"{}varz{} — environment variable manager

{}USAGE:{}
  varz                     List all env vars (sorted)
  varz <PATTERN>           Search env vars (case-insensitive)
  varz set KEY VALUE       Set an env var (current session + persisted)
  varz unset KEY           Unset an env var
  varz --init [SHELL]      Print shell integration (bash/zsh/fish)
  varz --version           Show version
  varz --help              Show this help

{}SETUP:{}
  bash/zsh — add to ~/.zshrc or ~/.bashrc:
    eval "$(varz --init)"

  fish — add to ~/.config/fish/config.fish:
    varz --init fish | source

  This installs a shell function that applies set/unset changes to
  the current session via a file-based directive protocol (no eval
  at runtime). Re-source your shell config after upgrading.

{}EXAMPLES:{}
  varz OPEN                # find any var containing "OPEN"
  varz set FOO bar         # set FOO=bar right now, no restart needed
  varz unset FOO           # unset FOO
"#,
        BOLD, RESET, BOLD, RESET, BOLD, RESET, BOLD, RESET
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Highlight occurrences of `pattern` (case-insensitive) inside `text`
fn highlight(text: &str, pattern: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_pattern = pattern.to_lowercase();

    // If case folding changed byte lengths, highlighting positions won't map
    // correctly between the lowered and original strings. Fall back to plain.
    if lower_text.len() != text.len() || lower_pattern.len() != pattern.len() {
        return format!("{}{}", BOLD, text);
    }

    let mut result = String::new();
    let mut last = 0;

    while let Some(pos) = lower_text[last..].find(&lower_pattern) {
        let abs = last + pos;
        result.push_str(&format!("{}{}", BOLD, &text[last..abs]));
        result.push_str(&format!(
            "{}{}{}{}",
            YELLOW,
            BOLD,
            &text[abs..abs + pattern.len()],
            RESET
        ));
        last = abs + pattern.len();
    }
    result.push_str(&format!("{}{}", BOLD, &text[last..]));
    result
}

/// Mask values that look like secrets
fn mask_if_secret(key: &str, value: &str) -> String {
    if NEVER_SECRET_KEYS.contains(&key) {
        return value.to_string();
    }
    let key_lower = key.to_lowercase();
    let is_secret = key_lower.contains("key")
        || key_lower.contains("secret")
        || key_lower.contains("token")
        || key_lower.contains("password")
        || key_lower.contains("passwd")
        || key_lower.contains("pwd")
        || key_lower.contains("auth");

    if is_secret {
        let char_count = value.chars().count();
        if char_count > MASK_EDGE_CHARS * 2 {
            let prefix_end = value
                .char_indices()
                .nth(MASK_EDGE_CHARS)
                .map_or(value.len(), |(i, _)| i);
            let suffix_start = value
                .char_indices()
                .rev()
                .nth(MASK_EDGE_CHARS - 1)
                .map_or(0, |(i, _)| i);
            let prefix = &value[..prefix_end];
            let suffix = &value[suffix_start..];
            format!("{}...{} {}(masked){}", prefix, suffix, DIM, RESET)
        } else if !value.is_empty() {
            // Short secret: never reveal value or its length
            format!("**** {}(masked){}", DIM, RESET)
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    }
}

/// Write a directive line to the mailbox file at `$VARZ_MAILBOX`.
///
/// Returns `true` if the directive was written successfully, `false` if
/// `VARZ_MAILBOX` is not set or the file could not be opened.
fn write_directive(directive: &str) -> bool {
    let Some(path) = env::var_os("VARZ_MAILBOX") else {
        return false;
    };
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        eprintln!("{}warning:{} failed to write to mailbox", YELLOW, RESET);
        return false;
    };
    // v1 header — duplicates are harmless; the shell function ignores them.
    let _ = writeln!(file, "v1");
    let _ = writeln!(file, "{directive}");
    true
}

/// Quote a value for safe use in shell export
fn shell_quote(value: &str) -> String {
    // Wrap in single quotes, escaping any single quotes within
    let escaped = value.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

/// Basic check: env var names must be alphanumeric + underscore, not start with digit
fn is_valid_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    match bytes.next() {
        None => return false,
        Some(b) if b.is_ascii_digit() => return false,
        _ => {}
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Persist KEY=VALUE to ~/.varz_env
fn persist_set(key: &str, value: &str) {
    let path = varz_env_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    // Remove old line for this key if present
    let filtered: String = existing
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start_matches("export ");
            !trimmed.starts_with(&format!("{}=", key))
        })
        .map(|l| format!("{}\n", l))
        .collect();

    let new_line = format!("export {}={}\n", key, shell_quote(value));
    let content = format!("{}{}", filtered, new_line);
    let _ = std::fs::write(&path, content);
    set_owner_only_permissions(&path);
}

/// Remove KEY from ~/.varz_env
fn persist_unset(key: &str) {
    let path = varz_env_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let filtered: String = existing
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start_matches("export ");
            !trimmed.starts_with(&format!("{}=", key))
        })
        .map(|l| format!("{}\n", l))
        .collect();
    let _ = std::fs::write(&path, filtered);
    set_owner_only_permissions(&path);
}

fn varz_env_path() -> std::path::PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".varz_env")
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &std::path::Path) {
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(unix)]
fn warn_if_loose_permissions() {
    let path = varz_env_path();
    if let Ok(metadata) = std::fs::metadata(&path) {
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            eprintln!(
                "{}warning:{} ~/.varz_env is readable by others, run: chmod 600 ~/.varz_env",
                YELLOW, RESET
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_valid_key ─────────────────────────────────────────────────────────

    #[test]
    fn valid_key_simple() {
        assert!(is_valid_key("FOO"));
    }

    #[test]
    fn valid_key_with_underscore() {
        assert!(is_valid_key("MY_VAR_123"));
    }

    #[test]
    fn valid_key_leading_underscore() {
        assert!(is_valid_key("_PRIVATE"));
    }

    #[test]
    fn invalid_key_empty() {
        assert!(!is_valid_key(""));
    }

    #[test]
    fn invalid_key_starts_with_digit() {
        assert!(!is_valid_key("1FOO"));
    }

    #[test]
    fn invalid_key_contains_hyphen() {
        assert!(!is_valid_key("MY-VAR"));
    }

    #[test]
    fn invalid_key_contains_space() {
        assert!(!is_valid_key("MY VAR"));
    }

    #[test]
    fn invalid_key_contains_equals() {
        assert!(!is_valid_key("MY=VAR"));
    }

    #[test]
    fn invalid_key_unicode_letters() {
        assert!(!is_valid_key("café"));
    }

    #[test]
    fn invalid_key_unicode_digit() {
        assert!(!is_valid_key("VAR٣"));
    }

    // ── shell_quote ──────────────────────────────────────────────────────────

    #[test]
    fn shell_quote_plain_value() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn shell_quote_value_with_spaces() {
        assert_eq!(shell_quote("hello world"), "'hello world'");
    }

    #[test]
    fn shell_quote_value_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_quote_empty_value() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_quote_value_with_dollar_sign() {
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
    }

    // ── mask_if_secret ───────────────────────────────────────────────────────

    #[test]
    fn mask_secret_key_long_value() {
        let result = mask_if_secret("API_KEY", "abcdefghijklmnop");
        assert!(result.contains("abcd"));
        assert!(result.contains("mnop"));
        assert!(result.contains("masked"));
        assert!(!result.contains("efghijkl"));
    }

    #[test]
    fn mask_secret_token_long_value() {
        let result = mask_if_secret("GITHUB_TOKEN", "ghp_1234567890abcdef");
        assert!(result.contains("masked"));
    }

    #[test]
    fn mask_secret_short_value() {
        let result = mask_if_secret("MY_PASSWORD", "hi");
        assert!(result.contains("****"));
        assert!(result.contains("masked"));
        assert!(!result.contains("hi"));
    }

    #[test]
    fn mask_secret_empty_value() {
        let result = mask_if_secret("SECRET_KEY", "");
        assert_eq!(result, "");
    }

    #[test]
    fn no_mask_for_non_secret() {
        let result = mask_if_secret("HOME", "/Users/dennis");
        assert_eq!(result, "/Users/dennis");
    }

    #[test]
    fn no_mask_for_pwd() {
        let result = mask_if_secret("PWD", "/home/user/projects");
        assert_eq!(result, "/home/user/projects");
    }

    #[test]
    fn no_mask_for_oldpwd() {
        let result = mask_if_secret("OLDPWD", "/home/user");
        assert_eq!(result, "/home/user");
    }

    #[test]
    fn mask_secret_non_ascii_value() {
        let result = mask_if_secret("API_KEY", "abcé12345678");
        assert!(result.contains("abcé"));
        assert!(result.contains("5678"));
        assert!(result.contains("masked"));
        assert!(!result.contains("abcé12345678"));
    }

    #[test]
    fn mask_detects_auth_in_key() {
        let result = mask_if_secret("ROVER_AUTHENTICATION", "some_long_secret_value_here");
        assert!(result.contains("masked"));
    }

    #[test]
    fn mask_detects_pwd_in_key() {
        let result = mask_if_secret("DB_PWD", "supersecret12345");
        assert!(result.contains("masked"));
    }

    // ── highlight ────────────────────────────────────────────────────────────

    #[test]
    fn highlight_marks_match() {
        let result = highlight("AWS_ACCESS_KEY", "aws");
        // YELLOW+BOLD wraps the matched portion
        assert!(result.contains(YELLOW));
        assert!(result.contains(BOLD));
        assert!(result.contains("AWS"));
    }

    #[test]
    fn highlight_no_match_returns_bold_base() {
        let result = highlight("HOME", "XYZ");
        assert!(result.contains(BOLD));
        assert!(result.contains("HOME"));
        assert!(!result.contains(YELLOW));
    }

    #[test]
    fn highlight_case_insensitive() {
        let result = highlight("MyToken", "token");
        assert!(result.contains(YELLOW));
    }

    #[test]
    fn highlight_multiple_matches() {
        let result = highlight("KEY_KEY", "KEY");
        // Both occurrences should be highlighted
        assert_eq!(result.matches(YELLOW).count(), 2);
    }

    // ── init scripts ─────────────────────────────────────────────────────────

    #[test]
    fn posix_init_contains_expected_constructs() {
        let s = posix_init_script();
        assert!(s.contains("eval \"$(varz --init)\""), "missing eval hint");
        assert!(s.contains("export \"$key=$val\""), "missing export");
        assert!(s.contains("__varz_apply"), "missing apply fn");
        assert!(s.contains(".varz_env"), "missing env load");
    }

    #[test]
    fn fish_init_contains_expected_constructs() {
        let s = fish_init_script();
        assert!(
            s.contains("varz --init fish | source"),
            "missing source hint"
        );
        assert!(s.contains("set -gx"), "missing set -gx");
        assert!(s.contains("__varz_apply"), "missing apply fn");
        assert!(s.contains(".varz_env"), "missing env load");
        assert!(s.contains("fish_pid"), "missing fish_pid");
    }

    #[test]
    fn fish_init_handles_unset_directive() {
        let s = fish_init_script();
        assert!(s.contains("case UNSET"), "missing UNSET case");
        assert!(s.contains("set -e"), "missing set -e for unset");
    }

    // ── write_directive ─────────────────────────────────────────────────────

    // SAFETY: these tests mutate process-wide env vars. They are safe in
    // single-threaded test execution (`cargo nextest` runs each test in its
    // own process).

    #[test]
    fn write_directive_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let mbox = dir.path().join("pending");
        // SAFETY: single-threaded test process (nextest).
        unsafe { env::set_var("VARZ_MAILBOX", &mbox) };
        assert!(write_directive("SET FOO bar"));
        let contents = std::fs::read_to_string(&mbox).unwrap();
        assert!(contents.contains("v1\n"));
        assert!(contents.contains("SET FOO bar\n"));
        unsafe { env::remove_var("VARZ_MAILBOX") };
    }

    #[test]
    fn write_directive_appends() {
        let dir = tempfile::tempdir().unwrap();
        let mbox = dir.path().join("pending");
        // SAFETY: single-threaded test process (nextest).
        unsafe { env::set_var("VARZ_MAILBOX", &mbox) };
        write_directive("SET A 1");
        write_directive("SET B 2");
        let contents = std::fs::read_to_string(&mbox).unwrap();
        assert!(contents.contains("SET A 1\n"));
        assert!(contents.contains("SET B 2\n"));
        unsafe { env::remove_var("VARZ_MAILBOX") };
    }

    #[test]
    fn write_directive_missing_mailbox_returns_false() {
        // SAFETY: single-threaded test process (nextest).
        unsafe { env::remove_var("VARZ_MAILBOX") };
        assert!(!write_directive("SET X y"));
    }

    #[test]
    fn write_directive_value_with_spaces_and_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let mbox = dir.path().join("pending");
        // SAFETY: single-threaded test process (nextest).
        unsafe { env::set_var("VARZ_MAILBOX", &mbox) };
        assert!(write_directive("SET GREETING hello world it's me"));
        let contents = std::fs::read_to_string(&mbox).unwrap();
        assert!(contents.contains("SET GREETING hello world it's me\n"));
        unsafe { env::remove_var("VARZ_MAILBOX") };
    }
}
