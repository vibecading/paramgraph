//! Wavefront OBJ export: one object per box, grouped by grade, so Rhino's
//! import puts measured, ruled and assumed geometry on separate layers. The
//! diff export groups by change instead: added, removed, moved (new place) and
//! unchanged, ready to colour green, red, amber and grey.

use crate::model::Elem;

fn push_box(out: &mut String, e: &Elem, base: &mut usize) {
    let (a, b) = (e.min, e.max);
    for &(x, y, z) in &[
        (a[0], a[1], a[2]), (b[0], a[1], a[2]), (b[0], b[1], a[2]), (a[0], b[1], a[2]),
        (a[0], a[1], b[2]), (b[0], a[1], b[2]), (b[0], b[1], b[2]), (a[0], b[1], b[2]),
    ] {
        out.push_str(&format!("v {x:.6} {y:.6} {z:.6}\n"));
    }
    let f = [[1, 4, 3, 2], [5, 6, 7, 8], [1, 2, 6, 5], [2, 3, 7, 6], [3, 4, 8, 7], [4, 1, 5, 8]];
    for q in f {
        out.push_str(&format!("f {} {} {} {}\n", *base + q[0], *base + q[1], *base + q[2], *base + q[3]));
    }
    *base += 8;
}

/// Each element goes into the group named by `group(e)`.
pub fn write(elems: &[Elem], group: impl Fn(&Elem) -> String) -> String {
    let mut out = String::from("# paramgraph export, metres, z up\n");
    let mut base = 0;
    for e in elems {
        out.push_str(&format!("o {}\ng {}\n", e.id, group(e)));
        push_box(&mut out, e, &mut base);
    }
    out
}

pub fn grade_name(g: char) -> &'static str {
    match g {
        'P' => "published",
        'D' => "measured",
        'I' => "rule",
        _ => "assumed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn a_box_has_8_vertices_and_6_faces() {
        let e = Elem { id: "b".into(), grade: 'D', min: [0.0; 3], max: [1.0, 2.0, 3.0], deps: BTreeSet::new() };
        let s = write(&[e.clone(), e], |e| grade_name(e.grade).into());
        assert_eq!(s.lines().filter(|l| l.starts_with("v ")).count(), 16);
        assert_eq!(s.lines().filter(|l| l.starts_with("f ")).count(), 12);
        assert!(s.contains("f 9 12 11 10"));
        assert!(s.contains("g measured"));
    }
}
