//! Equivalence transforms: given one input, produce other inputs that denote the
//! SAME real effect but are spelled differently. A guard that reads the label
//! (the text) instead of the box (what the machine does) will disagree on them.
//!
//! The families here mirror the "stringly-typed guardrails" bug class:
//!   - PATH   : ~/x, $HOME/x, ${HOME}/x, "$HOME"/x, absolute /home/<user>/x ...
//!   - SECRET : "--flag value" vs "--flag=value" vs "-f value" (space vs '=')

/// One rewritten input plus why it is equivalent to the base.
#[derive(Clone)]
pub struct Variant {
    pub family: &'static str,
    pub input: String,
    pub note: String,
}

/// Generate all variants for `base`. `home` is the absolute home dir used to
/// build the absolute-path spelling (e.g. "/home/ana").
pub fn variants(base: &str, home: &str) -> Vec<Variant> {
    let mut out = Vec::new();
    out.extend(path_variants(base, home));
    out.extend(secret_variants(base));
    // de-dup: drop any variant equal to the base or to an earlier variant
    let mut seen = std::collections::HashSet::new();
    seen.insert(base.to_string());
    out.retain(|v| seen.insert(v.input.clone()));
    out
}

// ---------------------------------------------------------------------------
// PATH family
// ---------------------------------------------------------------------------

/// Markers that introduce a home-relative path, longest first so matching is greedy.
const HOME_MARKERS: &[&str] = &["\"$HOME\"/", "${HOME}/", "$HOME/", "~/"];

/// If `base` contains a home-relative path, return every equivalent spelling of it.
fn path_variants(base: &str, home: &str) -> Vec<Variant> {
    let mut out = Vec::new();
    let home = home.trim_end_matches('/');

    // Find the first home marker and extract the tail (everything up to whitespace).
    let (marker_start, marker) = match HOME_MARKERS
        .iter()
        .filter_map(|m| base.find(m).map(|i| (i, *m)))
        .min_by_key(|(i, _)| *i)
    {
        Some(x) => x,
        None => return out,
    };

    let tail_start = marker_start + marker.len();
    let tail: String = base[tail_start..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    if tail.is_empty() {
        return out;
    }
    let token_end = tail_start + tail.len();
    let before = &base[..marker_start];
    let after = &base[token_end..];

    // Every spelling of the same resolved path. The quoted form keeps the path
    // literal while still expanding $HOME, which trips naive regexes.
    let spellings = [
        (format!("~/{tail}"), "tilde home"),
        (format!("$HOME/{tail}"), "unquoted $HOME (common regex miss)"),
        (format!("${{HOME}}/{tail}"), "braced ${HOME}"),
        (format!("\"$HOME\"/{tail}"), "quoted $HOME, literal path"),
        (format!("~/\"{tail}\""), "quoted tail after tilde"),
        (format!("{home}/{tail}"), "absolute path (no home marker at all)"),
    ];

    for (spelling, why) in spellings {
        out.push(Variant {
            family: "PATH",
            input: format!("{before}{spelling}{after}"),
            note: format!("same file, spelled as: {why}"),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// SECRET family
// ---------------------------------------------------------------------------

/// Flag names that usually carry a secret.
const SECRET_FLAG_HINTS: &[&str] = &[
    "password", "passwd", "pwd", "pass", "token", "secret", "apikey", "api-key",
    "api_key", "auth", "credential", "cred", "key",
];

/// Known credential prefixes (OpenAI, GitHub, Google, AWS, Slack, Stripe...).
const SECRET_VALUE_PREFIXES: &[&str] =
    &["sk-", "sk_live_", "sk_test_", "ghp_", "github_pat_", "gho_", "AKIA", "AIza", "xox"];

/// Does `flag`/`value` look like it carries a secret? Keeps the SECRET family
/// from firing on ordinary args like `-u root`.
fn looks_secret(flag: &str, value: &str) -> bool {
    let f = flag.trim_start_matches('-').to_ascii_lowercase();
    if SECRET_FLAG_HINTS.iter().any(|h| f.contains(h)) {
        return true;
    }
    if SECRET_VALUE_PREFIXES.iter().any(|p| value.starts_with(p)) {
        return true;
    }
    // high-entropy-ish: long and mixing letter cases or digits
    value.len() >= 16
        && value.chars().any(|c| c.is_ascii_uppercase())
        && value.chars().any(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Detect `--flag value`, `--flag=value` or `-f value` carrying a secret and emit
/// the other placements. A redactor keyed on `flag=value` misses the spaced form.
fn secret_variants(base: &str) -> Vec<Variant> {
    let mut out = Vec::new();
    let toks: Vec<&str> = base.split_whitespace().collect();

    for i in 0..toks.len() {
        let t = toks[i];
        // Case A: "--flag=value" -> split form "--flag value"
        if t.starts_with("--") {
            if let Some(eq) = t.find('=') {
                let flag = &t[..eq];
                let val = &t[eq + 1..];
                if !val.is_empty() && looks_secret(flag, val) {
                    let mut rebuilt = toks.clone();
                    rebuilt[i] = "\u{0}"; // placeholder; replaced below
                    let joined = rebuilt.join(" ").replacen("\u{0}", &format!("{flag} {val}"), 1);
                    out.push(Variant {
                        family: "SECRET",
                        input: joined,
                        note: format!("secret moved from '{flag}=X' to '{flag} X' (space, no '=')"),
                    });
                }
                continue;
            }
        }
        // Case B: "--flag value" or "-f value" -> joined form "--flag=value"
        let is_flag = (t.starts_with("--") && t.len() > 2) || (t.starts_with('-') && t.len() == 2);
        if is_flag && i + 1 < toks.len() {
            let val = toks[i + 1];
            if !val.starts_with('-') && !val.is_empty() && looks_secret(t, val) {
                let mut rebuilt: Vec<String> = toks.iter().map(|s| s.to_string()).collect();
                rebuilt[i] = format!("{t}={val}");
                rebuilt.remove(i + 1);
                out.push(Variant {
                    family: "SECRET",
                    input: rebuilt.join(" "),
                    note: format!("secret moved from '{t} X' to '{t}=X'"),
                });
            }
        }
    }
    out
}
