//! Every program door 2's scripts call from the hourly pass (`scripts/safe-push.sh`) must be on
//! the unit's path, `nix/services.nix`'s `doorTwo`, which `nix/module-check.nix` holds to the
//! triage unit's evaluated path.

use std::collections::BTreeSet;

use super::repo;

/// The scripts door 2 runs, in the order each calls the next.
const SCRIPTS: [&str; 3] = [
    "scripts/roadmap-push.sh",
    "scripts/public-scan.sh",
    "scripts/public-tree.sh",
];

/// What a script may run that the hourly pass never reaches: `public-scan.sh` reads a pull
/// request with `gh` only when it is handed one, and `roadmap-push.sh` never hands one.
const NOT_FROM_THE_PASS: [&str; 1] = ["gh"];

/// Words that start a command and are not a program on the path.
const SHELL: [&str; 46] = [
    "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "until", "case",
    "esac", "function", "select", "time", "!", "{", "}", "[[", "]]", "[", "echo", "printf", "cd",
    "local", "set", "shift", "trap", "exit", "return", "read", "declare", "export", "unset",
    "wait", "true", "false", "test", "mapfile", "readonly", "eval", "source", "pwd", "break",
    "continue",
];

/// Words after which the next word is the command: keywords, and `exec`, `command`, `xargs`.
const LEADING: [&str; 12] = [
    "then", "do", "else", "elif", "if", "while", "until", "!", "{", "exec", "command", "xargs",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A line's end, where a command starts.
    Newline,
    /// A place a command starts within a line: `;`, `|`, `&`, `(`, `$(`, `<(`.
    Start,
    /// A pattern's end in a `case`, `)` outside any substitution: no word of its line is a
    /// command.
    Pattern,
    /// Text that is no command word: a quoted string, an arithmetic expansion.
    Quoted,
    Word(String),
}

/// The script's tokens, with every `$(…)` inside a double-quoted string read as commands.
fn tokens(text: &str) -> Vec<Token> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum In {
        Code,
        Single,
        Double,
    }
    let bytes = text.as_bytes();
    let mut stack = vec![In::Code];
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let state = *stack.last().unwrap_or(&In::Code);
        match state {
            In::Single => {
                if c == b'\'' {
                    stack.pop();
                }
                i += 1;
            }
            In::Double => {
                if c == b'\\' {
                    i += 2;
                } else if c == b'"' {
                    stack.pop();
                    i += 1;
                } else if text[i..].starts_with("$(") && !text[i..].starts_with("$((") {
                    stack.push(In::Code);
                    out.push(Token::Start);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            In::Code => {
                let at_word = i == 0 || b" \t\n;".contains(&bytes[i - 1]);
                if c == b'#' && at_word {
                    i = text[i..].find('\n').map_or(bytes.len(), |end| i + end);
                } else if c == b'\'' || c == b'"' {
                    out.push(Token::Quoted);
                    stack.push(if c == b'\'' { In::Single } else { In::Double });
                    i += 1;
                } else if c == b'\\' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 2;
                } else if text[i..].starts_with("$((") || text[i..].starts_with("((") {
                    let mut depth = 0;
                    let mut j = i;
                    while j < bytes.len() {
                        match bytes[j] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    out.push(Token::Quoted);
                    i = j + 1;
                } else if text[i..].starts_with("$(") || c == b'(' {
                    // A substitution, a subshell or an array: its `)` closes it, never a pattern.
                    stack.push(In::Code);
                    out.push(Token::Start);
                    i += if c == b'(' { 1 } else { 2 };
                } else if c == b')' {
                    if stack.len() > 1 {
                        stack.pop();
                        out.push(Token::Quoted);
                    } else {
                        out.push(Token::Pattern);
                    }
                    i += 1;
                } else if c == b'\n' {
                    out.push(Token::Newline);
                    i += 1;
                } else if b";|&".contains(&c) {
                    out.push(Token::Start);
                    i += 1;
                } else if c == b' ' || c == b'\t' {
                    i += 1;
                } else {
                    let end = text[i..]
                        .find(|ch: char| " \t\n;|&()'\"".contains(ch))
                        .map_or(bytes.len(), |end| i + end.max(1));
                    out.push(Token::Word(text[i..end].to_owned()));
                    i = end;
                }
            }
        }
    }
    out
}

/// The programs `text` runs by name: every command word that is no keyword, builtin, function of
/// the script or variable assignment.
fn programs(text: &str) -> BTreeSet<String> {
    let functions: BTreeSet<&str> = text
        .lines()
        .filter_map(|line| line.trim_start().split_once("()").map(|(name, _)| name))
        .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    let program = |word: &str| {
        word.starts_with(|c: char| c.is_ascii_lowercase())
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.+-".contains(c))
            && !SHELL.contains(&word)
            && !functions.contains(word)
    };
    let mut found: Vec<(String, usize)> = Vec::new();
    let mut line = 0;
    let mut expecting = true;
    for token in tokens(text) {
        match token {
            Token::Newline => {
                line += 1;
                expecting = true;
            }
            Token::Start => expecting = true,
            Token::Pattern => {
                found.retain(|(_, at)| *at != line);
                expecting = true;
            }
            Token::Quoted => expecting = false,
            Token::Word(word) => {
                if !expecting {
                    continue;
                }
                let assignment = word.split_once('=').is_some_and(|(name, _)| {
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                });
                if assignment || LEADING.contains(&word.as_str()) {
                    continue;
                }
                expecting = false;
                if program(&word) {
                    found.push((word, line));
                }
            }
        }
    }
    found.into_iter().map(|(word, _)| word).collect()
}

/// The names `nix/services.nix`'s `doorTwo` gives programs.
fn declared() -> BTreeSet<String> {
    let nix = std::fs::read_to_string(repo().join("nix/services.nix")).expect("services.nix reads");
    let block = nix
        .split_once("doorTwo = with pkgs; {")
        .expect("services.nix declares doorTwo")
        .1
        .split_once("\n  };")
        .expect("doorTwo closes")
        .0;
    block
        .lines()
        .filter_map(|line| {
            line.trim()
                .split_once(" = ")
                .map(|(name, _)| name.to_owned())
        })
        .collect()
}

/// Every program door 2's scripts run from the hourly pass is one `doorTwo` puts on the unit's
/// path, and `doorTwo` names nothing they do not run.
#[test]
fn door_two_runs_only_what_the_hourly_pass_has() {
    let mut run = BTreeSet::new();
    for script in SCRIPTS {
        let text = std::fs::read_to_string(repo().join(script)).expect("a script reads");
        run.extend(programs(&text));
    }
    for pass_never in NOT_FROM_THE_PASS {
        assert!(
            run.remove(pass_never),
            "{pass_never} is no longer run: drop it"
        );
    }
    assert_eq!(
        run,
        declared(),
        "nix/services.nix's doorTwo is what the scripts run"
    );
}

/// The reading finds a program in each place a script calls one, and no keyword, function,
/// assignment, quoted word or `case` pattern.
#[test]
fn door_two_reads_the_commands_a_script_runs() {
    let script = "#!/usr/bin/env bash\n# a comment naming nothing\nsay() { printf '%s' \"$1\"; }\n\
                  x=\"$(alpha --flag | bravo)\"\nif charlie; then delta; fi\n\
                  case \"$x\" in\n  echo.toml | objects/*) echo yes ;;\nesac\n\
                  value=$((1 + 2))\nLC_ALL=C echo2 \"say hi (now)\" && say\nexec foxtrot\n\
                  (cd /tmp && golf \\\n  --long)\n";
    assert_eq!(
        programs(script),
        [
            "alpha", "bravo", "charlie", "delta", "echo2", "foxtrot", "golf"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
    );
}
