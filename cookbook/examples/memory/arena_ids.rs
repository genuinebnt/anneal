//! RECIPE 08 (memory): arena of nodes addressed by typed ids (no Box per node).
//! LEVEL: simple
//! Use in: stage 8 (allocation), and later the DSL / bytecode compiler.   Run: cargo run --example arena_ids
//! COMMON USES: compiler ASTs, game entities, graph nodes: allocate in one Vec and refer by index (oxc, rustc use arenas).
//!
//! Pattern: store all nodes in one `Vec`; children are `ExprId`s, not `Box<Expr>`. One allocation grows
//! the whole tree, nodes are cache-friendly, dropping is a single free, and there are no borrow-checker
//! fights over parent/child references.
//! Real code: oxc's bump arena -> https://github.com/oxc-project/oxc/tree/main/crates/oxc_allocator
#![allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct ExprId(u32);

#[derive(Debug)]
enum Expr {
    Num(f64),
    Add(ExprId, ExprId),
    Mul(ExprId, ExprId),
}

#[derive(Default)]
struct Arena(Vec<Expr>);

impl Arena {
    fn alloc(&mut self, e: Expr) -> ExprId {
        self.0.push(e);
        ExprId(self.0.len() as u32 - 1)
    }
    fn eval(&self, id: ExprId) -> f64 {
        match &self.0[id.0 as usize] {
            Expr::Num(n) => *n,
            Expr::Add(a, b) => self.eval(*a) + self.eval(*b),
            Expr::Mul(a, b) => self.eval(*a) * self.eval(*b),
        }
    }
}

fn main() {
    let mut arena = Arena::default();
    // (2 + 3) * 4
    let two = arena.alloc(Expr::Num(2.0));
    let three = arena.alloc(Expr::Num(3.0));
    let four = arena.alloc(Expr::Num(4.0));
    let sum = arena.alloc(Expr::Add(two, three));
    let root = arena.alloc(Expr::Mul(sum, four));
    println!("(2 + 3) * 4 = {}", arena.eval(root));
    println!("{} nodes in one Vec", arena.0.len());
}
