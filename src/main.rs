//! out-spell — try to defeat your agent's guardrail with synonyms.
//!
//! A stringly-typed guardrail decides from the surface text (the command string,
//! a path as written, a secret as spelled) instead of the ground truth (the file
//! actually read, the real action). out-spell generates equivalent spellings of
//! an input and flags the ones your guard decides DIFFERENTLY — those are the gaps.
//!
//! Background: https://dev.to/cleiton_augusto_/stringly-typed-guardrails-how-agent-safety-checks-fail-by-matching-the-wrong-thing-2kef

mod guard;
mod transforms;

use guard::Decision;
use std::process::exit;

const HELP: &str = "\
out-spell — try to defeat your agent's guardrail with synonyms

USAGE:
    out-spell check [OPTIONS] <INPUT>     Run a guard on <INPUT> + its equivalents, flag gaps
    out-spell transforms [OPTIONS] <INPUT>  Just print the equivalent spellings
    out-spell demo                        Run against a built-in stringly-typed guard

OPTIONS:
    --guard <CMD>   Command that scores one input. '{}' is replaced by the input;
                    with no '{}', the input is sent on stdin. The command should
                    print a decision (allow/block/ask/redact) or use its exit code
                    (0 = allow, non-zero = block). Default: a built-in demo guard.
    --home <PATH>   Home dir used to build the absolute-path spelling. Default: $HOME.
    --json          Machine-readable output.
    -h, --help      This help.

EXIT CODE:
    check exits non-zero when a gap is found (a protective base decision that a
    synonym escapes) — handy in CI.

EXAMPLE:
    out-spell demo
    out-spell check --guard './myguard {}' 'cat ~/.ssh/id_rsa'
";

struct Args {
    cmd: String,
    input: Option<String>,
    guard: Option<String>,
    home: String,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = std::env::args().skip(1);
    let cmd = a.next().unwrap_or_default();
    if cmd.is_empty() || cmd == "-h" || cmd == "--help" {
        println!("{HELP}");
        exit(0);
    }
    let home_default = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    let mut args = Args { cmd, input: None, guard: None, home: home_default, json: false };
    while let Some(tok) = a.next() {
        match tok.as_str() {
            "--guard" => args.guard = Some(a.next().ok_or("--guard needs a value")?),
            "--home" => args.home = a.next().ok_or("--home needs a value")?,
            "--json" => args.json = true,
            "-h" | "--help" => {
                println!("{HELP}");
                exit(0);
            }
            _ if tok.starts_with('-') => return Err(format!("unknown option: {tok}")),
            _ => args.input = Some(tok),
        }
    }
    Ok(args)
}

/// Ask the chosen guard (external command or the built-in demo) for a decision.
fn decide(args: &Args, input: &str) -> Decision {
    match &args.guard {
        Some(cmd) => guard::ask(cmd, input).unwrap_or(Decision::Unknown),
        None => guard::demo_guard(input),
    }
}

struct Row {
    family: &'static str,
    input: String,
    note: String,
    decision: Decision,
    gap: bool,
}

fn run_check(args: &Args, base: &str) -> i32 {
    let base_decision = decide(args, base);
    let variants = transforms::variants(base, &args.home);

    let mut rows = Vec::new();
    for v in &variants {
        let d = decide(args, &v.input);
        // A gap: the base was protective, the synonym is not (it slips past).
        let gap = base_decision.is_protective() && !d.is_protective();
        rows.push(Row { family: v.family, input: v.input.clone(), note: v.note.clone(), decision: d, gap });
    }
    let gaps = rows.iter().filter(|r| r.gap).count();

    if args.json {
        print_json(base, base_decision, &rows, gaps);
    } else {
        print_report(base, base_decision, &rows, gaps);
    }
    if gaps > 0 { 1 } else { 0 }
}

fn print_report(base: &str, base_decision: Decision, rows: &[Row], gaps: usize) {
    println!("base:    {base}");
    println!("decision: {}\n", base_decision.as_str());
    if !base_decision.is_protective() {
        println!("note: the base decision is not protective ({}), so there is nothing to escape.", base_decision.as_str());
        println!("      give out-spell an input your guard is supposed to stop.\n");
    }
    println!("{:<7}  {:<8}  {:<4}  {}", "FAMILY", "DECISION", "GAP", "INPUT");
    println!("{}", "-".repeat(72));
    for r in rows {
        let mark = if r.gap { "<==" } else { "" };
        println!("{:<7}  {:<8}  {:<4}  {}", r.family, r.decision.as_str(), mark, r.input);
    }
    println!();
    if gaps == 0 {
        println!("OK: every synonym got the same protection as the base ({} variants).", rows.len());
    } else {
        println!("FOUND {gaps} gap(s): a synonym of the same action escaped the guard.");
        println!("Your guard is reading the label, not the box. Details:");
        for r in rows.iter().filter(|r| r.gap) {
            println!("  - [{}] {} -> {}", r.family, r.decision.as_str(), r.note);
        }
    }
}

fn print_json(base: &str, base_decision: Decision, rows: &[Row], gaps: usize) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    print!("{{\"base\":\"{}\",\"base_decision\":\"{}\",\"gaps\":{},\"variants\":[",
        esc(base), base_decision.as_str(), gaps);
    for (i, r) in rows.iter().enumerate() {
        if i > 0 { print!(","); }
        print!("{{\"family\":\"{}\",\"decision\":\"{}\",\"gap\":{},\"input\":\"{}\",\"note\":\"{}\"}}",
            r.family, r.decision.as_str(), r.gap, esc(&r.input), esc(&r.note));
    }
    println!("]}}");
}

fn run_transforms(args: &Args, base: &str) {
    for v in transforms::variants(base, &args.home) {
        println!("[{}] {}\n      ({})", v.family, v.input, v.note);
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{HELP}");
            exit(2);
        }
    };

    match args.cmd.as_str() {
        "check" => {
            let base = args.input.clone().unwrap_or_else(|| {
                eprintln!("error: check needs an <INPUT>\n");
                exit(2);
            });
            exit(run_check(&args, &base));
        }
        "transforms" => {
            let base = args.input.clone().unwrap_or_else(|| {
                eprintln!("error: transforms needs an <INPUT>\n");
                exit(2);
            });
            run_transforms(&args, &base);
        }
        "demo" => {
            println!("# demo: a built-in guard that blocks '~/.ssh/id_rsa' but reads the label, not the box.\n");
            let demo = Args { guard: None, ..args };
            exit(run_check(&demo, "cat ~/.ssh/id_rsa"));
        }
        other => {
            eprintln!("error: unknown command '{other}'\n\n{HELP}");
            exit(2);
        }
    }
}
