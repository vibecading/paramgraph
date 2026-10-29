//! The model file: a building as named parameters, derived values and boxes.
//!
//! One statement per line, so a git diff of two versions reads as a list of
//! design changes:
//!
//! ```text
//! model "Farnsworth House"
//! param s = 6.706 P "column spacing, HABS sheet 3"
//! let L = 2*c + 3*s "length: two cantilevers and three bays"
//! box floor D: x 0 .. L, y 0 .. W, z zf_b .. zf_t
//! box col_S[i in 0..2] D: x c + i*s - col/2 .. c + i*s + col/2, y -col .. 0, z 0 .. zr_t
//! check L = 23.57 tol 0.05 "HABS sheet 3: 77'-4 1/8\""
//! ```
//!
//! Grades: P read off a drawing or a published dimension, D measured from a
//! photo, I inferred by a rule, A assumed. A `let` is a rule, grade I.

use crate::expr::Expr;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub value: f64,
    pub grade: char,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct Let {
    pub name: String,
    pub expr: Expr,
    pub src: String,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct BoxDef {
    pub id: String,
    pub repeat: Option<(String, Expr, Expr)>,
    pub grade: char,
    pub axes: [(Expr, Expr); 3],
    pub src: String,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub expr: Expr,
    pub src: String,
    pub value: f64,
    pub tol: Option<f64>,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub name: String,
    pub params: Vec<Param>,
    pub lets: Vec<Let>,
    pub boxes: Vec<BoxDef>,
    pub checks: Vec<Check>,
}

/// A box after evaluation, with the parameters it depends on.
#[derive(Debug, Clone, PartialEq)]
pub struct Elem {
    pub id: String,
    pub grade: char,
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub deps: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub src: String,
    pub value: f64,
    pub expected: f64,
    pub tol: Option<f64>,
    pub pass: Option<bool>,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct Evaluated {
    pub env: BTreeMap<String, f64>,
    pub elems: Vec<Elem>,
    pub checks: Vec<CheckResult>,
    /// for every derived name and box, the parameters it ultimately depends on
    pub deps: BTreeMap<String, BTreeSet<String>>,
}

/// Split a trailing `"note"` off a statement. Inside the note `\"` is a quote
/// (inches: `22'-0\"`).
fn split_note(s: &str) -> (String, String) {
    let s = s.trim();
    let b = s.as_bytes();
    if s.len() >= 2 && b[b.len() - 1] == b'"' && b[b.len() - 2] != b'\\' {
        let mut i = b.len() - 1;
        while i > 0 {
            i -= 1;
            if b[i] == b'"' && (i == 0 || b[i - 1] != b'\\') {
                return (s[..i].trim().to_string(), s[i + 1..s.len() - 1].replace("\\\"", "\""));
            }
        }
    }
    (s.to_string(), String::new())
}

/// Cut a `#` comment, but not a `#` inside a note.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut prev = ' ';
    for (i, ch) in line.char_indices() {
        if ch == '"' && prev != '\\' {
            quoted = !quoted;
        } else if ch == '#' && !quoted {
            return &line[..i];
        }
        prev = ch;
    }
    line
}

/// split at commas that are not inside parentheses
fn top_level_split(s: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut depth = 0;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(ch);
    }
    out
}

fn range(s: &str) -> Result<(Expr, Expr), String> {
    let (a, b) = s.split_once("..").ok_or_else(|| format!("expected 'a .. b', got '{s}'"))?;
    Ok((Expr::parse(a)?, Expr::parse(b)?))
}

fn grade(s: &str) -> Result<char, String> {
    match s.trim() {
        g @ ("P" | "D" | "I" | "A") => Ok(g.chars().next().unwrap()),
        g => Err(format!("grade must be P, D, I or A, got '{g}'")),
    }
}

impl Model {
    pub fn parse(text: &str) -> Result<Model, String> {
        let mut m = Model::default();
        for (n, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            m.statement(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        }
        m.check_names()?;
        Ok(m)
    }

    fn statement(&mut self, line: &str) -> Result<(), String> {
        let (kw, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match kw {
            "model" => self.name = rest.trim_matches('"').to_string(),
            "param" => {
                let (body, note) = split_note(rest);
                let (name, rhs) = body.split_once('=').ok_or("expected 'param name = value GRADE'")?;
                let mut it = rhs.split_whitespace();
                let value = it.next().ok_or("missing value")?.parse::<f64>().map_err(|e| e.to_string())?;
                let g = grade(it.next().ok_or("missing grade")?)?;
                self.params.push(Param { name: name.trim().into(), value, grade: g, note });
            }
            "let" => {
                let (body, note) = split_note(rest);
                let (name, rhs) = body.split_once('=').ok_or("expected 'let name = expression'")?;
                self.lets.push(Let { name: name.trim().into(), expr: Expr::parse(rhs)?, src: rhs.trim().into(), note });
            }
            "box" => {
                let (head, body) = rest.split_once(':').ok_or("expected 'box id GRADE: x a..b, y a..b, z a..b'")?;
                let (idpart, g) = head.trim().rsplit_once(char::is_whitespace).ok_or("missing grade")?;
                let idpart = idpart.trim();
                let (id, repeat) = match idpart.split_once('[') {
                    Some((id, r)) => {
                        let r = r.strip_suffix(']').ok_or("missing ']'")?;
                        let (var, rng) = r.split_once(" in ").ok_or("expected '[i in a..b]'")?;
                        let (a, b) = range(rng)?;
                        (id.to_string(), Some((var.trim().to_string(), a, b)))
                    }
                    None => (idpart.to_string(), None),
                };
                let parts = top_level_split(body);
                if parts.len() != 3 {
                    return Err("a box needs x, y and z ranges".into());
                }
                let mut axes = Vec::new();
                for (p, axis) in parts.iter().zip(["x", "y", "z"]) {
                    let p = p.trim();
                    let r = p.strip_prefix(axis).ok_or_else(|| format!("expected '{axis} a .. b', got '{p}'"))?;
                    axes.push(range(r)?);
                }
                let axes: [(Expr, Expr); 3] = axes.try_into().unwrap();
                self.boxes.push(BoxDef { id, repeat, grade: grade(g)?, axes, src: body.trim().into() });
            }
            "check" => {
                let (body, note) = split_note(rest);
                let (lhs, rhs) = body.rsplit_once('=').ok_or("expected 'check expression = value [tol t]'")?;
                let mut it = rhs.split_whitespace();
                let value = it.next().ok_or("missing value")?.parse::<f64>().map_err(|e| e.to_string())?;
                let tol = match (it.next(), it.next()) {
                    (Some("tol"), Some(t)) => Some(t.parse::<f64>().map_err(|e| e.to_string())?),
                    (None, _) => None,
                    _ => return Err("expected 'tol <metres>' after the value".into()),
                };
                self.checks.push(Check { expr: Expr::parse(lhs)?, src: lhs.trim().into(), value, tol, note });
            }
            _ => return Err(format!("unknown statement '{kw}'")),
        }
        Ok(())
    }

    fn check_names(&self) -> Result<(), String> {
        let mut seen = BTreeSet::new();
        for n in self.params.iter().map(|p| &p.name).chain(self.lets.iter().map(|l| &l.name)) {
            if !seen.insert(n.clone()) {
                return Err(format!("'{n}' is defined twice"));
            }
        }
        let mut ids = BTreeSet::new();
        for b in &self.boxes {
            if !ids.insert(b.id.clone()) {
                return Err(format!("box '{}' is defined twice", b.id));
            }
        }
        Ok(())
    }

    /// Order the lets so each comes after the names it uses; a cycle is an error
    /// that names its members.
    fn let_order(&self) -> Result<Vec<usize>, String> {
        let idx: BTreeMap<&str, usize> = self.lets.iter().enumerate().map(|(i, l)| (l.name.as_str(), i)).collect();
        let mut state = vec![0u8; self.lets.len()]; // 0 new, 1 on stack, 2 done
        let mut order = Vec::new();
        fn visit(i: usize, m: &Model, idx: &BTreeMap<&str, usize>, st: &mut [u8], order: &mut Vec<usize>, path: &mut Vec<String>) -> Result<(), String> {
            if st[i] == 2 {
                return Ok(());
            }
            path.push(m.lets[i].name.clone());
            if st[i] == 1 {
                return Err(format!("rules refer to each other in a cycle: {}", path.join(" -> ")));
            }
            st[i] = 1;
            for n in m.lets[i].expr.names() {
                if let Some(&j) = idx.get(n.as_str()) {
                    visit(j, m, idx, st, order, path)?;
                }
            }
            st[i] = 2;
            path.pop();
            order.push(i);
            Ok(())
        }
        for i in 0..self.lets.len() {
            visit(i, self, &idx, &mut state, &mut order, &mut Vec::new())?;
        }
        Ok(order)
    }

    pub fn eval(&self) -> Result<Evaluated, String> {
        let mut env: BTreeMap<String, f64> = self.params.iter().map(|p| (p.name.clone(), p.value)).collect();
        let mut deps: BTreeMap<String, BTreeSet<String>> =
            self.params.iter().map(|p| (p.name.clone(), BTreeSet::from([p.name.clone()]))).collect();
        let closure = |names: BTreeSet<String>, deps: &BTreeMap<String, BTreeSet<String>>| -> BTreeSet<String> {
            names.iter().flat_map(|n| deps.get(n).cloned().unwrap_or_default()).collect()
        };
        for i in self.let_order()? {
            let l = &self.lets[i];
            let v = l.expr.eval(&env).map_err(|e| format!("let {}: {e}", l.name))?;
            env.insert(l.name.clone(), v);
            deps.insert(l.name.clone(), closure(l.expr.names(), &deps));
        }
        let mut elems = Vec::new();
        for b in &self.boxes {
            let names: BTreeSet<String> = b.axes.iter().flat_map(|(a, c)| a.names().into_iter().chain(c.names())).collect();
            let d = closure(names, &deps);
            deps.insert(b.id.clone(), d.clone());
            let instances: Vec<(String, Option<(String, f64)>)> = match &b.repeat {
                None => vec![(b.id.clone(), None)],
                Some((var, a, c)) => {
                    let (a, c) = (a.eval(&env)?.round() as i64, c.eval(&env)?.round() as i64);
                    (a..c).map(|i| (format!("{}[{i}]", b.id), Some((var.clone(), i as f64)))).collect()
                }
            };
            for (id, bind) in instances {
                let mut e = env.clone();
                if let Some((var, v)) = bind {
                    e.insert(var, v);
                }
                let mut min = [0.0; 3];
                let mut max = [0.0; 3];
                for k in 0..3 {
                    let lo = b.axes[k].0.eval(&e).map_err(|err| format!("box {id}: {err}"))?;
                    let hi = b.axes[k].1.eval(&e).map_err(|err| format!("box {id}: {err}"))?;
                    min[k] = lo.min(hi);
                    max[k] = lo.max(hi);
                }
                elems.push(Elem { id, grade: b.grade, min, max, deps: d.clone() });
            }
        }
        let checks = self
            .checks
            .iter()
            .map(|c| {
                let v = c.expr.eval(&env)?;
                Ok(CheckResult {
                    src: c.src.clone(),
                    value: v,
                    expected: c.value,
                    tol: c.tol,
                    pass: c.tol.map(|t| (v - c.value).abs() <= t),
                    note: c.note.clone(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Evaluated { env, elems, checks, deps })
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"
model "test"
param a = 2 P "drawing"
param b = 3 D
let c = a * b      # a rule
let d = c + 1
box slab D: x 0 .. d, y 0 .. b, z a .. a + 0.5
box col[i in 0..3] I: x i*a .. i*a + 0.2, y 0 .. 0.2, z 0 .. a
check d = 7 tol 0.01 "by construction"
"#;

    #[test]
    fn parses_and_evaluates() {
        let m = Model::parse(SMALL).unwrap();
        assert_eq!(m.name, "test");
        let e = m.eval().unwrap();
        assert_eq!(e.env["d"], 7.0);
        assert_eq!(e.elems.len(), 4);
        assert_eq!(e.elems[0].max, [7.0, 3.0, 2.5]);
        assert_eq!(e.elems[3].id, "col[2]");
        assert_eq!(e.elems[3].min[0], 4.0);
        assert_eq!(e.checks[0].pass, Some(true));
        assert_eq!(e.deps["slab"], BTreeSet::from(["a".to_string(), "b".to_string()]));
        assert_eq!(e.deps["col"], BTreeSet::from(["a".to_string()]));
    }

    #[test]
    fn lets_in_any_order() {
        let m = Model::parse("param a = 1 P\nlet c = b + 1\nlet b = a * 2\n").unwrap();
        assert_eq!(m.eval().unwrap().env["c"], 3.0);
    }

    #[test]
    fn cycle_is_named() {
        let m = Model::parse("let a = b + 1\nlet b = a * 2\n").unwrap();
        let err = m.eval().unwrap_err();
        assert!(err.contains("cycle") && err.contains("a -> b -> a"), "{err}");
    }

    #[test]
    fn errors_carry_line_numbers() {
        let err = Model::parse("param a = 1 P\nparam b = 2 X\n").unwrap_err();
        assert!(err.starts_with("line 2"), "{err}");
        let err = Model::parse("param a = 1 P\nlet a = 2\n").unwrap_err();
        assert!(err.contains("twice"), "{err}");
        let err = Model::parse("box s D: x 0 .. 1, y 0 .. 1").unwrap_err();
        assert!(err.contains("x, y and z"), "{err}");
    }

    #[test]
    fn unknown_name_in_box() {
        let m = Model::parse("box s D: x 0 .. q, y 0 .. 1, z 0 .. 1").unwrap();
        assert!(m.eval().unwrap_err().contains("unknown name 'q'"));
    }

    #[test]
    fn notes_with_inches_and_hashes() {
        let m = Model::parse("param s = 6.706 P \"bay, 22'-0\\\" (sheet #3)\"  # comment\n").unwrap();
        assert_eq!(m.params[0].note, "bay, 22'-0\" (sheet #3)");
        assert_eq!(m.params[0].value, 6.706);
    }

    #[test]
    fn commas_inside_functions() {
        let m = Model::parse("param a = 1 P\nbox s D: x 0 .. max(a, 2), y 0 .. 1, z 0 .. 1").unwrap();
        assert_eq!(m.eval().unwrap().elems[0].max[0], 2.0);
    }
}
