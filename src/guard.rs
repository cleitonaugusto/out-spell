//! Ask a guardrail for its decision on one input.
//!
//! The guard is EXTERNAL and plugin-specific, so out-spell talks to it through a
//! small command you provide. The candidate input is given to that command either
//! substituted for a `{}` placeholder, or on stdin. The decision is read from the
//! command's stdout (a keyword like allow/block/ask/redact) or, if none is found,
//! from its exit code (0 = allow, non-zero = block).

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Allow,
    Block,
    Ask,
    Redact,
    Unknown,
}

impl Decision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Decision::Allow => "ALLOW",
            Decision::Block => "BLOCK",
            Decision::Ask => "ASK",
            Decision::Redact => "REDACT",
            Decision::Unknown => "UNKNOWN",
        }
    }

    fn parse(s: &str) -> Option<Decision> {
        let s = s.trim().to_ascii_lowercase();
        let first = s.split_whitespace().next().unwrap_or("");
        match first {
            "allow" | "pass" | "ok" | "permit" => Some(Decision::Allow),
            "block" | "deny" | "refuse" | "stop" | "blocked" => Some(Decision::Block),
            "ask" | "approve" | "confirm" | "prompt" => Some(Decision::Ask),
            "redact" | "scrub" | "mask" => Some(Decision::Redact),
            _ => None,
        }
    }

    /// Is this decision protective (would stop or soften the dangerous effect)?
    pub fn is_protective(&self) -> bool {
        matches!(self, Decision::Block | Decision::Ask | Decision::Redact)
    }
}

/// Run `guard_cmd` against `input`. `{}` in the command is replaced by the input;
/// if there is no `{}`, the input is sent on stdin.
pub fn ask(guard_cmd: &str, input: &str) -> std::io::Result<Decision> {
    let use_stdin = !guard_cmd.contains("{}");
    let cmd_str = guard_cmd.replace("{}", input);

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&cmd_str);
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    if use_stdin {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn()?;
    if use_stdin {
        if let Some(mut si) = child.stdin.take() {
            let _ = si.write_all(input.as_bytes());
            let _ = si.write_all(b"\n");
        }
    }
    let out = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Prefer an explicit keyword in stdout; fall back to the exit code.
    if let Some(d) = Decision::parse(&stdout) {
        return Ok(d);
    }
    Ok(if out.status.success() {
        Decision::Allow
    } else {
        Decision::Block
    })
}

/// A built-in toy guard that is deliberately stringly-typed: it blocks a few
/// hard-coded path spellings and `--password X=...` shapes but misses the
/// equivalents. Used by `out-spell demo` so the tool works with nothing wired.
pub fn demo_guard(input: &str) -> Decision {
    // Blocks only the literal spellings it has "seen before".
    let blocked_paths = ["~/.ssh/id_rsa", "/etc/shadow"];
    for p in blocked_paths {
        if input.contains(p) {
            return Decision::Block;
        }
    }
    // Redacts only `key=value` secrets, not space-separated ones.
    if input.contains("password=") || input.contains("token=") {
        return Decision::Redact;
    }
    Decision::Allow
}
