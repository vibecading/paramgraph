//! Three-way merge of model files, with conflicts found through the graph.
//!
//! Statements are matched by identity (a param or let by name, a box by id, a
//! check by its expression), so reordering lines is not a change. Two kinds of
//! conflict come out:
//!
//! - **text**: both sides changed the same statement differently, or one changed
//!   what the other deleted. The merge keeps ours and reports it.
//! - **design**: the text merges cleanly, but an element depends on a change
//!   from each side (one moved the bay, the other the glass set-back, and the
//!   glass wall depends on both). Nobody has seen that combination; a reviewer
//!   should. Checks that pass on both sides and fail after the merge are
//!   reported the same way.

use crate::model::Model;
use std::collections::{BTreeMap, BTreeSet};

/// The identity of a statement line, e.g. "param s", "box col_S", "check L".
pub fn key(line: &str) -> Option<String> {
    let (kw, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    let name = match kw {
        "model" => String::new(),
        "param" | "let" => rest.split('=').next()?.trim().to_string(),
        "box" => rest.split(|c: char| c == '[' || c.is_whitespace() || c == ':').next()?.to_string(),
        "check" => {
            // the expression left of the last '=' outside the note
            let body = rest.rsplit_once('=')?.0;
            body.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        _ => return None,
    };
    Some(format!("{kw} {name}").trim().to_string())
}

fn statements(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') {
                return None;
            }
            key(t).map(|k| (k, t.to_string()))
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct Merge {
    pub text: String,
    pub text_conflicts: Vec<String>,
    pub design_conflicts: Vec<String>,
    pub broken_checks: Vec<String>,
    pub taken_from_theirs: Vec<String>,
}

impl Merge {
    pub fn clean(&self) -> bool {
        self.text_conflicts.is_empty() && self.design_conflicts.is_empty() && self.broken_checks.is_empty()
    }
}

pub fn merge(base: &str, ours: &str, theirs: &str) -> Result<Merge, String> {
    let (b, o, t) = (statements(base), statements(ours), statements(theirs));
    let bm: BTreeMap<_, _> = b.iter().cloned().collect();
    let om: BTreeMap<_, _> = o.iter().cloned().collect();
    let tm: BTreeMap<_, _> = t.iter().cloned().collect();
    let mut out = Merge::default();
    let mut picks: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut changed_ours = BTreeSet::new();
    let mut changed_theirs = BTreeSet::new();
    // walk ours in order, then append what only theirs added
    let mut keys: Vec<String> = o.iter().map(|(k, _)| k.clone()).collect();
    for (k, _) in &t {
        if !om.contains_key(k) && !bm.contains_key(k) {
            keys.push(k.clone());
        }
    }
    // statements both sides kept from base but ours deleted: decide below
    for (k, _) in &b {
        if !om.contains_key(k) && !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    for k in keys {
        let (bv, ov, tv) = (bm.get(&k), om.get(&k), tm.get(&k));
        if ov != bv {
            changed_ours.insert(k.clone());
        }
        if tv != bv {
            changed_theirs.insert(k.clone());
        }
        let pick = match (bv, ov, tv) {
            (_, o, t) if o == t => o,
            (b, o, t) if o == b => {
                out.taken_from_theirs.push(k.clone());
                t
            }
            (b, o, t) if t == b => o,
            (_, o, t) => {
                let what = match (o, t) {
                    (None, Some(_)) => "ours deleted it, theirs changed it",
                    (Some(_), None) => "ours changed it, theirs deleted it",
                    _ => "both changed it differently",
                };
                out.text_conflicts.push(format!("{k}: {what}; kept ours"));
                o
            }
        };
        picks.insert(k, pick.cloned());
    }
    // keep ours' layout (comments, blank lines, order) and swap statement lines
    let mut lines: Vec<String> = Vec::new();
    let mut done = BTreeSet::new();
    for raw in ours.lines() {
        match key(raw.trim()).filter(|_| !raw.trim().starts_with('#')) {
            Some(k) => {
                if let Some(Some(l)) = picks.get(&k) {
                    lines.push(l.clone());
                }
                done.insert(k);
            }
            None => lines.push(raw.to_string()),
        }
    }
    // statements only theirs has go after the last statement of the same kind
    for (k, l) in &picks {
        let Some(l) = l else { continue };
        if done.contains(k) {
            continue;
        }
        let kw = k.split(' ').next().unwrap_or("");
        let at = lines.iter().rposition(|x| x.trim_start().starts_with(&format!("{kw} "))).map_or(lines.len(), |i| i + 1);
        lines.insert(at, l.clone());
    }
    out.text = lines.join("\n") + "\n";

    // design conflicts: an element whose dependencies were changed on both sides
    let merged = Model::parse(&out.text).map_err(|e| format!("merged model: {e}"))?;
    let em = merged.eval().map_err(|e| format!("merged model: {e}"))?;
    // a statement key "param s" / "let L" names the graph node "s" / "L"
    let node = |k: &String| k.split_once(' ').map(|(_, n)| n.to_string()).unwrap_or_default();
    let names_o: BTreeSet<String> = changed_ours.iter().filter(|k| k.starts_with("param ") || k.starts_with("let ")).map(node).collect();
    let names_t: BTreeSet<String> = changed_theirs.iter().filter(|k| k.starts_with("param ") || k.starts_with("let ")).map(node).collect();
    // dependencies including lets, not only the parameters they reduce to
    let uses = |names: BTreeSet<String>| -> BTreeSet<String> {
        let mut all = names.clone();
        let mut frontier: Vec<String> = names.into_iter().collect();
        while let Some(n) = frontier.pop() {
            if let Some(l) = merged.lets.iter().find(|l| l.name == n) {
                for m in l.expr.names() {
                    if all.insert(m.clone()) {
                        frontier.push(m);
                    }
                }
            }
        }
        all
    };
    for bx in &merged.boxes {
        let direct: BTreeSet<String> = bx.axes.iter().flat_map(|(a, c)| a.names().into_iter().chain(c.names())).collect();
        let d = uses(direct);
        let by_o: Vec<_> = d.intersection(&names_o).cloned().collect();
        let by_t: Vec<_> = d.intersection(&names_t).cloned().collect();
        let both_same = by_o.iter().all(|n| by_t.contains(n)) && by_t.iter().all(|n| by_o.contains(n));
        if !by_o.is_empty() && !by_t.is_empty() && !both_same {
            out.design_conflicts.push(format!(
                "box {}: ours changed {}, theirs changed {}; the combination was never seen by either side",
                bx.id,
                by_o.join(", "),
                by_t.join(", ")
            ));
        }
    }
    // checks that hold on each side but fail on the merge
    let pass = |text: &str| -> BTreeMap<String, Option<bool>> {
        Model::parse(text)
            .and_then(|m| m.eval())
            .map(|e| e.checks.into_iter().map(|c| (c.src, c.pass)).collect())
            .unwrap_or_default()
    };
    let (po, pt) = (pass(ours), pass(theirs));
    for c in &em.checks {
        if c.pass == Some(false) && po.get(&c.src) == Some(&Some(true)) && pt.get(&c.src) == Some(&Some(true)) {
            out.broken_checks.push(format!("check {}: passes on both sides, fails merged ({:.3} vs {})", c.src, c.value, c.expected));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "param a = 2 P\nparam b = 3 D\nparam g = 0.2 D\nlet c = a * b\nbox slab D: x 0 .. c, y 0 .. 1, z 0 .. 1\nbox glass D: x g .. c - g, y 0 .. 1, z 1 .. 2\nbox post I: x 0 .. 0.2, y 0 .. 0.2, z 0 .. b\ncheck c = 6 tol 1.5\n";

    #[test]
    fn keys() {
        assert_eq!(key("param s = 6.7 P \"x\"").unwrap(), "param s");
        assert_eq!(key("box col_S[i in 0..2] D: x 0 .. 1, y 0 .. 1, z 0 .. 1").unwrap(), "box col_S");
        assert_eq!(key("check zr_b - zf_t = 2.87 tol 0.1 \"x\"").unwrap(), "check zr_b - zf_t");
    }

    #[test]
    fn disjoint_edits_merge_cleanly() {
        let ours = BASE.replace("param b = 3 D", "param b = 3.5 D");
        let theirs = BASE.replace("box post I", "box column I");
        let m = merge(BASE, &ours, &theirs).unwrap();
        assert!(m.text_conflicts.is_empty(), "{:?}", m.text_conflicts);
        assert!(m.text.contains("param b = 3.5 D") && m.text.contains("box column I"));
        assert!(!m.text.contains("box post"));
    }

    #[test]
    fn same_statement_both_sides_is_a_text_conflict() {
        let ours = BASE.replace("param a = 2 P", "param a = 2.1 P");
        let theirs = BASE.replace("param a = 2 P", "param a = 2.2 P");
        let m = merge(BASE, &ours, &theirs).unwrap();
        assert_eq!(m.text_conflicts.len(), 1);
        assert!(m.text.contains("param a = 2.1 P"));
    }

    #[test]
    fn element_fed_from_both_sides_is_a_design_conflict() {
        // ours moves the bay (through rule c), theirs the glass set-back
        let ours = BASE.replace("param a = 2 P", "param a = 2.4 P");
        let theirs = BASE.replace("param g = 0.2 D", "param g = 0.5 D");
        let m = merge(BASE, &ours, &theirs).unwrap();
        assert!(m.text_conflicts.is_empty());
        assert_eq!(m.design_conflicts.len(), 1, "{:?}", m.design_conflicts);
        assert!(m.design_conflicts[0].starts_with("box glass"));
    }

    #[test]
    fn check_broken_only_by_the_combination() {
        // each side raises c by 1.2: within 1.5 alone, 2.4 together
        let ours = BASE.replace("param a = 2 P", "param a = 2.4 P");
        let theirs = BASE.replace("param b = 3 D", "param b = 3.6 D");
        let m = merge(BASE, &ours, &theirs).unwrap();
        assert_eq!(m.broken_checks.len(), 1, "{:?}", m.broken_checks);
    }

    #[test]
    fn keeps_comments_and_places_additions() {
        let base = "# header\nparam a = 1 P\n\n# boxes\nbox s D: x 0 .. a, y 0 .. 1, z 0 .. 1\n";
        let ours = base.replace("param a = 1 P", "param a = 2 P");
        let theirs = base.replace("param a = 1 P\n", "param a = 1 P\nparam b = 5 A\n");
        let m = merge(base, &ours, &theirs).unwrap();
        assert_eq!(m.text, "# header\nparam a = 2 P\nparam b = 5 A\n\n# boxes\nbox s D: x 0 .. a, y 0 .. 1, z 0 .. 1\n");
    }

    #[test]
    fn modify_delete() {
        let ours = BASE.replace("param g = 0.2 D", "param g = 0.3 D");
        let theirs = BASE.replace("box glass D: x g .. c - g, y 0 .. 1, z 1 .. 2\n", "").replace("param g = 0.2 D\n", "");
        let m = merge(BASE, &ours, &theirs).unwrap();
        assert!(m.text_conflicts.iter().any(|c| c.contains("theirs deleted")), "{:?}", m.text_conflicts);
    }
}
