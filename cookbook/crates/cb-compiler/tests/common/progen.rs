//! The type-directed program generator shared by the property tests (`props.rs`)
//! and the back-end differential tests (`backend.rs`).
#![allow(dead_code)]

use super::Rng;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum T {
    Int,
    Bool,
    Str,
    Arr,
    Tup,
    P,
    E,
}
pub const ALL: [T; 7] = [T::Int, T::Bool, T::Str, T::Arr, T::Tup, T::P, T::E];

pub fn tname(t: T) -> &'static str {
    match t {
        T::Int => "int",
        T::Bool => "bool",
        T::Str => "str",
        T::Arr => "[int]",
        T::Tup => "(int, bool)",
        T::P => "P",
        T::E => "E",
    }
}

pub struct Sig {
    pub name: String,
    pub params: Vec<T>,
    pub ret: T,
}

pub struct Gen {
    pub rng: Rng,
    pub fns: Vec<Sig>,
    pub counter: usize,
}

pub type Env = Vec<(String, T)>;

impl Gen {
    pub fn fresh(&mut self) -> String {
        self.counter += 1;
        format!("v{}", self.counter)
    }

    pub fn expr(&mut self, t: T, depth: u32, env: &Env) -> String {
        let vars: Vec<&String> = env
            .iter()
            .filter(|(_, vt)| *vt == t)
            .map(|(n, _)| n)
            .collect();
        let calls: Vec<usize> = (0..self.fns.len())
            .filter(|&i| self.fns[i].ret == t)
            .collect();
        let choice = if depth == 0 {
            self.rng.below(2)
        } else {
            self.rng.below(10)
        };
        if choice == 0 && !vars.is_empty() {
            return (*self.rng.pick(&vars)).clone();
        }
        if choice >= 7 && !calls.is_empty() && depth > 0 {
            let i = *self.rng.pick(&calls);
            let params = self.fns[i].params.clone();
            let args: Vec<String> = params
                .iter()
                .map(|p| self.expr(*p, depth - 1, env))
                .collect();
            return format!("{}({})", self.fns[i].name, args.join(", "));
        }
        let d = depth.saturating_sub(1);
        match t {
            T::Int => match if depth == 0 { 0 } else { self.rng.below(8) } {
                0 => format!("{}", self.rng.below(20)),
                1 => format!(
                    "({} + {})",
                    self.expr(T::Int, d, env),
                    self.expr(T::Int, d, env)
                ),
                2 => format!(
                    "({} - {})",
                    self.expr(T::Int, d, env),
                    self.expr(T::Int, d, env)
                ),
                3 => format!(
                    "({} % {})",
                    self.expr(T::Int, d, env),
                    1 + self.rng.below(7)
                ),
                4 => format!(
                    "if ({}) {{ {} }} else {{ {} }}",
                    self.expr(T::Bool, d, env),
                    self.expr(T::Int, d, env),
                    self.expr(T::Int, d, env)
                ),
                5 => format!("len({})", self.expr(T::Arr, d, env)),
                6 => format!("{}.a", self.expr(T::P, d, env)),
                _ => format!("{}.0", self.expr(T::Tup, d, env)),
            },
            T::Bool => match if depth == 0 { 0 } else { self.rng.below(7) } {
                0 => ["true", "false"][self.rng.below(2)].to_string(),
                1 => format!(
                    "({} < {})",
                    self.expr(T::Int, d, env),
                    self.expr(T::Int, d, env)
                ),
                2 => format!(
                    "({} == {})",
                    self.expr(T::Int, d, env),
                    self.expr(T::Int, d, env)
                ),
                3 => format!(
                    "({} && {})",
                    self.expr(T::Bool, d, env),
                    self.expr(T::Bool, d, env)
                ),
                4 => format!("!{}", self.expr(T::Bool, d, env)),
                5 => format!("{}.b", self.expr(T::P, d, env)),
                _ => format!("{}.1", self.expr(T::Tup, d, env)),
            },
            T::Str => match if depth == 0 { 0 } else { self.rng.below(4) } {
                0 => format!("\"s{}\"", self.rng.below(9)),
                1 => format!(
                    "({} + {})",
                    self.expr(T::Str, d, env),
                    self.expr(T::Str, d, env)
                ),
                2 => format!("to_str({})", self.expr(T::Int, d, env)),
                _ => format!("to_str({})", self.expr(T::Bool, d, env)),
            },
            T::Arr => {
                let n = 1 + self.rng.below(3);
                let xs: Vec<String> = (0..n).map(|_| self.expr(T::Int, d, env)).collect();
                format!("[{}]", xs.join(", "))
            }
            T::Tup => format!(
                "({}, {})",
                self.expr(T::Int, d, env),
                self.expr(T::Bool, d, env)
            ),
            T::P => format!(
                "P {{ a: {}, b: {} }}",
                self.expr(T::Int, d, env),
                self.expr(T::Bool, d, env)
            ),
            T::E => match if depth == 0 { 0 } else { self.rng.below(4) } {
                0 => "E::A".to_string(),
                1 => format!("E::B({})", self.expr(T::Int, d, env)),
                2 => format!("E::C {{ x: {} }}", self.expr(T::Int, d, env)),
                _ => format!(
                    "if ({}) {{ E::A }} else {{ E::B(1) }}",
                    self.expr(T::Bool, d, env)
                ),
            },
        }
    }

    /// A `match` over an enum producing type `t` (exercises binding and exhaustiveness).
    fn match_expr(&mut self, t: T, depth: u32, env: &Env) -> String {
        let scrut = self.expr(T::E, depth, env);
        let (v, x) = (self.fresh(), self.fresh());
        let mut e1 = env.clone();
        e1.push((v.clone(), T::Int));
        let mut e2 = env.clone();
        e2.push((x.clone(), T::Int));
        format!(
            "match ({scrut}) {{ E::A => {}, E::B({v}) => {}, E::C {{ x: {x} }} => {} }}",
            self.expr(t, depth, env),
            self.expr(t, depth, &e1),
            self.expr(t, depth, &e2)
        )
    }

    pub fn program(&mut self) -> String {
        let mut out =
            String::from("struct P { a: int, b: bool }\nenum E { A, B(int), C { x: int } }\n");
        let nfns = 1 + self.rng.below(5);
        for i in 0..nfns {
            let np = self.rng.below(4);
            let params: Vec<T> = (0..np).map(|_| *self.rng.pick(&ALL)).collect();
            let ret = *self.rng.pick(&ALL);
            let mut env: Env = Vec::new();
            let mut ps = Vec::new();
            for p in &params {
                let n = self.fresh();
                ps.push(format!("{n}: {}", tname(*p)));
                env.push((n, *p));
            }
            let mut body = String::new();
            for _ in 0..self.rng.below(4) {
                let lt = *self.rng.pick(&ALL);
                let e = self.expr(lt, 3, &env);
                let n = self.fresh();
                body.push_str(&format!("    let {n} = {e};\n"));
                env.push((n, lt));
            }
            let tail = if self.rng.below(4) == 0 {
                self.match_expr(ret, 2, &env)
            } else {
                self.expr(ret, 3, &env)
            };
            let name = format!("f{i}");
            out.push_str(&format!(
                "fn {name}({}) -> {} {{\n{body}    {tail}\n}}\n",
                ps.join(", "),
                tname(ret)
            ));
            self.fns.push(Sig { name, params, ret });
        }
        out.push_str("fn main() {\n");
        for i in 0..self.fns.len() {
            let (name, params, ret) = (
                self.fns[i].name.clone(),
                self.fns[i].params.clone(),
                self.fns[i].ret,
            );
            let args: Vec<String> = params
                .iter()
                .map(|p| self.expr(*p, 2, &Vec::new()))
                .collect();
            let call = format!("{name}({})", args.join(", "));
            out.push_str(&match ret {
                T::Int => format!("    println(to_str({call}));\n"),
                T::Bool => format!("    println(to_str({call}));\n"),
                T::Str => format!("    println({call});\n"),
                T::Arr => format!("    println(to_str(len({call})));\n"),
                _ => format!("    let _r{i} = {call};\n"),
            });
        }
        out.push_str("}\n");
        out
    }
}
