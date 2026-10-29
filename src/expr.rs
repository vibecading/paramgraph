//! Arithmetic expressions over named parameters.
//!
//! Numbers, names, `+ - * / ^`, unary minus, parentheses and a few functions
//! (`sqrt abs min max sin cos tan atan2 round floor ceil`, angles in degrees).
//! `names()` lists the free names, which is how the dependency graph is built.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && i + 1 < c.len() && c[i + 1].is_ascii_digit()) {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                i += 1;
                if i < c.len() && (c[i] == '+' || c[i] == '-') {
                    i += 1;
                }
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number '{t}'"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(c[st..i].iter().collect()));
        } else if "+-*/^(),".contains(ch) {
            out.push(Tok::Op(ch));
            i += 1;
        } else {
            return Err(format!("unexpected '{ch}' in '{s}'"));
        }
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p)
    }
    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(&Tok::Op(op)) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    // precedence climbing: + - < * / < unary - < ^ (right associative)
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            let (op, prec, right) = match self.peek() {
                Some(Tok::Op(o @ ('+' | '-'))) => (*o, 1, false),
                Some(Tok::Op(o @ ('*' | '/'))) => (*o, 2, false),
                Some(Tok::Op('^')) => ('^', 4, true),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.p += 1;
            let rhs = self.expr(if right { prec } else { prec + 1 })?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat('-') {
            // -x^2 is -(x^2)
            let e = self.expr(3)?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        if self.eat('+') {
            return self.expr(3);
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Expr, String> {
        match self.t.get(self.p).cloned() {
            Some(Tok::Num(v)) => {
                self.p += 1;
                Ok(Expr::Num(v))
            }
            Some(Tok::Name(n)) => {
                self.p += 1;
                if self.eat('(') {
                    let mut args = Vec::new();
                    if !self.eat(')') {
                        loop {
                            args.push(self.expr(0)?);
                            if self.eat(')') {
                                break;
                            }
                            if !self.eat(',') {
                                return Err(format!("expected ',' or ')' in call to {n}"));
                            }
                        }
                    }
                    Ok(Expr::Call(n, args))
                } else {
                    Ok(Expr::Var(n))
                }
            }
            Some(Tok::Op('(')) => {
                self.p += 1;
                let e = self.expr(0)?;
                if !self.eat(')') {
                    return Err("missing ')'".into());
                }
                Ok(e)
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

impl Expr {
    pub fn parse(s: &str) -> Result<Expr, String> {
        let mut p = Parser { t: lex(s)?, p: 0 };
        let e = p.expr(0)?;
        if p.p != p.t.len() {
            return Err(format!("trailing input in '{s}'"));
        }
        Ok(e)
    }

    pub fn names(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Var(n) => {
                out.insert(n.clone());
            }
            Expr::Neg(e) => e.collect(out),
            Expr::Bin(_, a, b) => {
                a.collect(out);
                b.collect(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.collect(out)),
        }
    }

    pub fn eval(&self, env: &BTreeMap<String, f64>) -> Result<f64, String> {
        Ok(match self {
            Expr::Num(v) => *v,
            Expr::Var(n) => *env.get(n).ok_or_else(|| format!("unknown name '{n}'"))?,
            Expr::Neg(e) => -e.eval(env)?,
            Expr::Bin(op, a, b) => {
                let (x, y) = (a.eval(env)?, b.eval(env)?);
                match op {
                    '+' => x + y,
                    '-' => x - y,
                    '*' => x * y,
                    '/' => x / y,
                    '^' => x.powf(y),
                    _ => unreachable!(),
                }
            }
            Expr::Call(f, args) => {
                let v: Vec<f64> = args.iter().map(|a| a.eval(env)).collect::<Result<_, _>>()?;
                let one = |g: fn(f64) -> f64| -> Result<f64, String> {
                    if v.len() == 1 {
                        Ok(g(v[0]))
                    } else {
                        Err(format!("{f}() takes one argument"))
                    }
                };
                match f.as_str() {
                    "sqrt" => one(f64::sqrt)?,
                    "abs" => one(f64::abs)?,
                    "round" => one(f64::round)?,
                    "floor" => one(f64::floor)?,
                    "ceil" => one(f64::ceil)?,
                    "sin" => one(|d| d.to_radians().sin())?,
                    "cos" => one(|d| d.to_radians().cos())?,
                    "tan" => one(|d| d.to_radians().tan())?,
                    "min" if !v.is_empty() => v.iter().cloned().fold(f64::INFINITY, f64::min),
                    "max" if !v.is_empty() => v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    "atan2" if v.len() == 2 => v[0].atan2(v[1]).to_degrees(),
                    _ => return Err(format!("unknown function {f}() with {} arguments", v.len())),
                }
            }
        })
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Num(v) => write!(f, "{v}"),
            Expr::Var(n) => write!(f, "{n}"),
            Expr::Neg(e) => write!(f, "-({e})"),
            Expr::Bin(op, a, b) => write!(f, "({a} {op} {b})"),
            Expr::Call(n, args) => {
                write!(f, "{n}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str, env: &[(&str, f64)]) -> f64 {
        let m = env.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        Expr::parse(s).unwrap().eval(&m).unwrap()
    }

    #[test]
    fn precedence() {
        assert_eq!(ev("2 + 3 * 4", &[]), 14.0);
        assert_eq!(ev("(2 + 3) * 4", &[]), 20.0);
        assert_eq!(ev("-2^2", &[]), -4.0);
        assert_eq!(ev("2^3^2", &[]), 512.0);
        assert_eq!(ev("10 - 4 - 3", &[]), 3.0);
        assert_eq!(ev("1.5e1 / 3", &[]), 5.0);
    }

    #[test]
    fn names_and_functions() {
        assert_eq!(ev("2*c + 3*s", &[("c", 1.724), ("s", 6.706)]), 2.0 * 1.724 + 3.0 * 6.706);
        assert_eq!(ev("max(a, b, 1)", &[("a", 2.0), ("b", 5.0)]), 5.0);
        assert!((ev("cos(60)", &[]) - 0.5).abs() < 1e-12);
        let e = Expr::parse("2*c + 3*s - g_x").unwrap();
        assert_eq!(e.names().into_iter().collect::<Vec<_>>(), vec!["c", "g_x", "s"]);
    }

    #[test]
    fn errors() {
        assert!(Expr::parse("2 +").is_err());
        assert!(Expr::parse("(1").is_err());
        assert!(Expr::parse("1 $ 2").is_err());
        assert!(Expr::parse("x").unwrap().eval(&BTreeMap::new()).is_err());
        assert!(Expr::parse("foo(1)").unwrap().eval(&BTreeMap::new()).is_err());
    }
}
