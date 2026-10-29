//! What changed between two versions of a model, in design terms.
//!
//! The text diff says which lines changed; this says what that did: which
//! parameters moved and by how much, which boxes appeared, disappeared or
//! moved, which parameter changes caused each move, and which checks flipped.

use crate::model::{Evaluated, Model};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: Kind,
    pub what: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub params: Vec<Line>,
    pub rules: Vec<Line>,
    pub elems: Vec<Line>,
    pub checks: Vec<Line>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub moved: Vec<String>,
}

const EPS: f64 = 1e-9;

fn fmt_box(min: &[f64; 3], max: &[f64; 3]) -> String {
    format!("x {:.3}..{:.3}, y {:.3}..{:.3}, z {:.3}..{:.3}", min[0], max[0], min[1], max[1], min[2], max[2])
}

pub fn diff(a: &Model, ea: &Evaluated, b: &Model, eb: &Evaluated) -> Diff {
    let mut d = Diff::default();
    let mut moved_params = BTreeSet::new();
    for p in &a.params {
        match b.param(&p.name) {
            None => d.params.push(Line { kind: Kind::Removed, what: p.name.clone(), detail: format!("{} {}", p.value, p.grade) }),
            Some(q) => {
                let mut parts = Vec::new();
                if (q.value - p.value).abs() > EPS {
                    parts.push(format!("{} -> {} ({:+.3})", p.value, q.value, q.value - p.value));
                    moved_params.insert(p.name.clone());
                }
                if q.grade != p.grade {
                    parts.push(format!("grade {} -> {}", p.grade, q.grade));
                }
                if !parts.is_empty() {
                    d.params.push(Line { kind: Kind::Changed, what: p.name.clone(), detail: parts.join(", ") });
                }
            }
        }
    }
    for q in &b.params {
        if a.param(&q.name).is_none() {
            d.params.push(Line { kind: Kind::Added, what: q.name.clone(), detail: format!("{} {}", q.value, q.grade) });
            moved_params.insert(q.name.clone());
        }
    }
    let la: BTreeMap<_, _> = a.lets.iter().map(|l| (l.name.clone(), l)).collect();
    let lb: BTreeMap<_, _> = b.lets.iter().map(|l| (l.name.clone(), l)).collect();
    let mut changed_rules = BTreeSet::new();
    for (n, l) in &la {
        match lb.get(n) {
            None => d.rules.push(Line { kind: Kind::Removed, what: n.clone(), detail: l.src.clone() }),
            Some(m) if m.expr != l.expr => {
                d.rules.push(Line { kind: Kind::Changed, what: n.clone(), detail: format!("{} -> {}", l.src, m.src) });
                changed_rules.insert(n.clone());
            }
            _ => {}
        }
    }
    for (n, m) in &lb {
        if !la.contains_key(n) {
            d.rules.push(Line { kind: Kind::Added, what: n.clone(), detail: m.src.clone() });
            changed_rules.insert(n.clone());
        }
    }
    let xa: BTreeMap<_, _> = ea.elems.iter().map(|e| (e.id.clone(), e)).collect();
    let xb: BTreeMap<_, _> = eb.elems.iter().map(|e| (e.id.clone(), e)).collect();
    for (id, e) in &xa {
        match xb.get(id) {
            None => {
                d.elems.push(Line { kind: Kind::Removed, what: id.clone(), detail: fmt_box(&e.min, &e.max) });
                d.removed.push(id.clone());
            }
            Some(f) => {
                let delta = (0..3)
                    .flat_map(|k| [(f.min[k] - e.min[k]).abs(), (f.max[k] - e.max[k]).abs()])
                    .fold(0.0, f64::max);
                if delta > EPS || f.grade != e.grade {
                    let mut because: Vec<String> = f.deps.intersection(&moved_params).cloned().collect();
                    // a rule whose text changed moves its boxes without any parameter moving
                    let base = id.split('[').next().unwrap_or(id);
                    let rules_used: BTreeSet<String> = b
                        .boxes
                        .iter()
                        .filter(|x| x.id == base)
                        .flat_map(|x| x.axes.iter().flat_map(|(p, q)| p.names().into_iter().chain(q.names())))
                        .collect();
                    because.extend(rules_used.intersection(&changed_rules).map(|r| format!("rule {r}")));
                    let mut detail = format!("moved up to {delta:.3} m");
                    if f.grade != e.grade {
                        detail += &format!(", grade {} -> {}", e.grade, f.grade);
                    }
                    if !because.is_empty() {
                        detail += &format!(" because of {}", because.join(", "));
                    }
                    d.elems.push(Line { kind: Kind::Changed, what: id.clone(), detail });
                    d.moved.push(id.clone());
                }
            }
        }
    }
    for (id, f) in &xb {
        if !xa.contains_key(id) {
            d.elems.push(Line { kind: Kind::Added, what: id.clone(), detail: fmt_box(&f.min, &f.max) });
            d.added.push(id.clone());
        }
    }
    let ca: BTreeMap<_, _> = ea.checks.iter().map(|c| (c.src.clone(), c)).collect();
    for c in &eb.checks {
        let before = ca.get(&c.src).and_then(|x| x.pass);
        if before != c.pass || (ca.get(&c.src).map_or(true, |x| (x.value - c.value).abs() > EPS)) {
            let state = |p: Option<bool>| match p {
                Some(true) => "pass",
                Some(false) => "FAIL",
                None => "-",
            };
            d.checks.push(Line {
                kind: if ca.contains_key(&c.src) { Kind::Changed } else { Kind::Added },
                what: c.src.clone(),
                detail: format!("{:.3} vs {} : {} -> {}", c.value, c.expected, state(before), state(c.pass)),
            });
        }
    }
    d
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.rules.is_empty() && self.elems.is_empty() && self.checks.is_empty()
    }

    pub fn render(&self, color: bool) -> String {
        let mut out = String::new();
        for (title, lines) in [("parameters", &self.params), ("rules", &self.rules), ("elements", &self.elems), ("checks", &self.checks)] {
            if lines.is_empty() {
                continue;
            }
            out += &format!("{title}\n");
            for l in lines {
                let (mark, c) = match l.kind {
                    Kind::Added => ('+', "\x1b[32m"),
                    Kind::Removed => ('-', "\x1b[31m"),
                    Kind::Changed => ('~', "\x1b[33m"),
                };
                let (c, r) = if color { (c, "\x1b[0m") } else { ("", "") };
                out += &format!("  {c}{mark} {}{r}  {}\n", l.what, l.detail);
            }
        }
        if out.is_empty() {
            out = "no design change\n".into();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> (Model, Evaluated) {
        let m = Model::parse(s).unwrap();
        let e = m.eval().unwrap();
        (m, e)
    }

    const A: &str = "param a = 2 P\nparam b = 3 D\nlet c = a * b\nbox slab D: x 0 .. c, y 0 .. 1, z 0 .. 1\nbox post I: x 0 .. 0.2, y 0 .. 0.2, z 0 .. b\nbox old A: x 0 .. 1, y 0 .. 1, z 0 .. 1\n";

    #[test]
    fn nothing_changed() {
        let (m, e) = ev(A);
        assert!(diff(&m, &e, &m, &e).is_empty());
    }

    #[test]
    fn param_moves_only_its_boxes() {
        let (m1, e1) = ev(A);
        let (m2, e2) = ev(&A.replace("param a = 2 P", "param a = 2.5 P"));
        let d = diff(&m1, &e1, &m2, &e2);
        assert_eq!(d.moved, vec!["slab"]);
        assert!(d.elems[0].detail.contains("because of a"), "{}", d.elems[0].detail);
    }

    #[test]
    fn added_removed_and_rule_change() {
        let (m1, e1) = ev(A);
        let b = A.replace("box old A: x 0 .. 1, y 0 .. 1, z 0 .. 1\n", "box new A: x 5 .. 6, y 0 .. 1, z 0 .. 1\n")
            .replace("let c = a * b", "let c = a * b + 1");
        let (m2, e2) = ev(&b);
        let d = diff(&m1, &e1, &m2, &e2);
        assert_eq!(d.added, vec!["new"]);
        assert_eq!(d.removed, vec!["old"]);
        assert_eq!(d.moved, vec!["slab"]);
        assert!(d.elems.iter().any(|l| l.detail.contains("rule c")));
        assert_eq!(d.rules.len(), 1);
    }
}
