use paramgraph::{diff, merge, model::Model, obj};
use std::collections::BTreeMap;
use std::process::ExitCode;

const USAGE: &str = "usage:
  pg eval  MODEL              parameters, rules, elements and checks (always exit 0)
  pg check MODEL              checks only; exit code 1 if any fails
  pg deps  MODEL [NAME]       what each parameter feeds, or what NAME depends on
  pg diff  OLD NEW [--color]  design changes between two versions
  pg obj   MODEL OUT.obj      boxes grouped by grade
  pg diff-obj OLD NEW OUT.obj added / removed / moved / unchanged groups
  pg merge BASE OURS THEIRS [OUT]  three-way merge; exit code 1 on any conflict";

fn load(path: &str) -> Result<Model, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    Model::parse(&text).map_err(|e| format!("{path}: {e}"))
}

fn run(args: &[String]) -> Result<bool, String> {
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match a.as_slice() {
        ["eval", path] => {
            let m = load(path)?;
            let e = m.eval()?;
            println!("{}", if m.name.is_empty() { *path } else { &m.name });
            for p in &m.params {
                println!("  param {:<10} {:>10.3}  {}  {}", p.name, p.value, p.grade, p.note);
            }
            for l in &m.lets {
                println!("  let   {:<10} {:>10.3}  I  {}", l.name, e.env[&l.name], l.src);
            }
            let mut by: BTreeMap<char, usize> = BTreeMap::new();
            for x in &e.elems {
                *by.entry(x.grade).or_default() += 1;
            }
            let counts: Vec<String> = by.iter().map(|(g, n)| format!("{n} {}", obj::grade_name(*g))).collect();
            println!("  {} boxes: {}", e.elems.len(), counts.join(", "));
            print_checks(&e.checks);
            // eval only shows; it also serves as git's diff textconv, which must not fail.
            // Gating on checks is `pg check`.
            Ok(true)
        }
        ["check", path] => {
            let e = load(path)?.eval()?;
            print_checks(&e.checks);
            Ok(e.checks.iter().all(|c| c.pass != Some(false)))
        }
        ["deps", path, rest @ ..] => {
            let m = load(path)?;
            let e = m.eval()?;
            match rest {
                [name] => {
                    let d = e.deps.get(*name).ok_or_else(|| format!("no rule or box named '{name}'"))?;
                    println!("{name} depends on: {}", d.iter().cloned().collect::<Vec<_>>().join(", "));
                }
                _ => {
                    for p in &m.params {
                        let fed: Vec<&str> = m.boxes.iter().filter(|b| e.deps[&b.id].contains(&p.name)).map(|b| b.id.as_str()).collect();
                        let flag = if fed.is_empty() { "  (feeds nothing)" } else { "" };
                        println!("{} {} -> {}{flag}", p.grade, p.name, fed.join(", "));
                    }
                }
            }
            Ok(true)
        }
        ["diff", old, new, rest @ ..] => {
            let (ma, mb) = (load(old)?, load(new)?);
            let d = diff::diff(&ma, &ma.eval()?, &mb, &mb.eval()?);
            print!("{}", d.render(rest.contains(&"--color")));
            Ok(true)
        }
        ["obj", path, out] => {
            let e = load(path)?.eval()?;
            std::fs::write(out, obj::write(&e.elems, |x| obj::grade_name(x.grade).into())).map_err(|e| e.to_string())?;
            println!("{} boxes -> {out}", e.elems.len());
            Ok(true)
        }
        ["diff-obj", old, new, out] => {
            let (ma, mb) = (load(old)?, load(new)?);
            let (ea, eb) = (ma.eval()?, mb.eval()?);
            let d = diff::diff(&ma, &ea, &mb, &eb);
            // removed boxes come from the old version, everything else from the new one
            let mut elems: Vec<_> = ea.elems.iter().filter(|x| d.removed.contains(&x.id)).cloned().collect();
            elems.extend(eb.elems.iter().cloned());
            let group = |x: &paramgraph::model::Elem| {
                if d.removed.contains(&x.id) {
                    "removed"
                } else if d.added.contains(&x.id) {
                    "added"
                } else if d.moved.contains(&x.id) {
                    "moved"
                } else {
                    "unchanged"
                }
                .to_string()
            };
            std::fs::write(out, obj::write(&elems, group)).map_err(|e| e.to_string())?;
            println!("{} added, {} removed, {} moved -> {out}", d.added.len(), d.removed.len(), d.moved.len());
            Ok(true)
        }
        ["merge", base, ours, theirs, rest @ ..] => {
            let read = |p: &str| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
            let m = merge::merge(&read(base)?, &read(ours)?, &read(theirs)?)?;
            for (title, list) in [("text conflicts", &m.text_conflicts), ("design conflicts", &m.design_conflicts), ("checks broken by the merge", &m.broken_checks)] {
                if !list.is_empty() {
                    println!("{title}");
                    list.iter().for_each(|c| println!("  {c}"));
                }
            }
            if !m.taken_from_theirs.is_empty() {
                println!("taken from theirs: {}", m.taken_from_theirs.join(", "));
            }
            if m.clean() {
                println!("clean merge");
            }
            match rest {
                [out] => std::fs::write(out, &m.text).map_err(|e| e.to_string())?,
                _ => print!("{}", m.text),
            }
            Ok(m.clean())
        }
        _ => Err(USAGE.into()),
    }
}

fn print_checks(checks: &[paramgraph::model::CheckResult]) {
    for c in checks {
        let err = c.value - c.expected;
        let pct = if c.expected != 0.0 { format!(" ({:+.1} %)", 100.0 * err / c.expected) } else { String::new() };
        let verdict = match c.pass {
            Some(true) => "pass".to_string(),
            Some(false) => format!("FAIL (tol {})", c.tol.unwrap()),
            None => "no tolerance".to_string(),
        };
        println!("  check {:<24} {:>9.3} vs {:>9.3}  {:+.3}{pct}  {verdict}  {}", c.src, c.value, c.expected, err, c.note);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
