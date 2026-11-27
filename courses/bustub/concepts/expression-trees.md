---
title: Expression trees: one node per operator, evaluated by recursion
summary: How a SQL expression becomes a tree of trait objects, how a node evaluates itself by asking its children, why trees are shared and rebuilt rather than edited, and how the optimizer inspects a node with a downcast.
minutes: 9
---
`(price - discount) * 2 > 100` is a tree: a `>` node whose children are a `*` node and the constant 100; the `*` node's children are a `-` node and the constant 2; the `-` node's children are two column references. An engine does not re-read the text for every row; it builds this tree once and **evaluates** it per row.

```svg
caption: The expression (a - c) + 100 > b as a tree. Evaluating the root asks each child for its value, bottom-up; the leaves read the row or hold a literal. A node never looks at the row itself unless it is a leaf.
<svg viewBox="0 0 760 250" role="img" aria-label="An expression tree with a comparison at the root">
<defs><marker id="et-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="blue" x="320" y="14" width="120" height="40" rx="3"/><text class="mid t-b" x="380" y="39">  &gt;  </text>
<rect class="live" x="170" y="86" width="120" height="40" rx="3"/><text class="mid fg" x="230" y="111">  +  </text>
<rect class="live" x="470" y="86" width="120" height="40" rx="3"/><text class="mid fg" x="530" y="111">column #0.1  (b)</text>
<rect class="live" x="70" y="158" width="120" height="40" rx="3"/><text class="mid fg" x="130" y="183">  -  </text>
<rect class="live" x="270" y="158" width="120" height="40" rx="3"/><text class="mid fg" x="330" y="183">constant 100</text>
<rect class="never" x="10" y="214" width="100" height="30" rx="3"/><text class="mid dim sm" x="60" y="234">#0.0 (a)</text>
<rect class="never" x="130" y="214" width="100" height="30" rx="3"/><text class="mid dim sm" x="180" y="234">#0.2 (c)</text>
<path class="ln" d="M340 54 L240 84" marker-end="url(#et-a)"/><path class="ln" d="M420 54 L520 84" marker-end="url(#et-a)"/>
<path class="ln" d="M210 126 L140 156" marker-end="url(#et-a)"/><path class="ln" d="M250 126 L320 156" marker-end="url(#et-a)"/>
<path class="ln" d="M100 198 L66 212" marker-end="url(#et-a)"/><path class="ln" d="M160 198 L176 212" marker-end="url(#et-a)"/>
</svg>
```

## One interface, many node types

Every node answers the same question, "what is your value for this row?", so the executor that holds the root does not care what is inside. In C++ this is a base class with a virtual `Evaluate`; in Rust it is a **trait**, and the children are **trait objects**:

```rust,ignore
trait Expression { fn evaluate(&self, row: &Row) -> Value; fn children(&self) -> &[Arc<dyn Expression>]; }
struct Plus { children: Vec<Arc<dyn Expression>> }       // two children
struct Column(usize);                                    // no children: a leaf
```

`Arc<dyn Expression>` is a shared pointer to *some* implementer: the compiler does not know which, and the call goes through a vtable. A node that holds its children as `Arc<dyn Expression>` can be built from any combination of nodes: that is what makes `(a - c) + 100` the same code as `lower(name)`.

## Evaluation is recursion

`Plus::evaluate` asks `children[0]` and `children[1]` for their values and adds them. Leaves return a constant or read the row. There is no loop over the tree and no explicit stack: the call stack is the traversal. The depth of the tree bounds the stack depth, and SQL expressions are shallow.

## Trees are shared and rebuilt, not edited

The optimizer wants to rewrite expressions (move a filter below a join and change which input each column refers to). With `Arc` nodes it **builds a new tree**, reusing the unchanged subtrees by cloning the pointer: `node.clone_with_children(new_children)` is "the same node, other children". Nothing is mutated, so a plan that is being executed can never be changed under the executor, and two plans can share subtrees safely.

## Looking inside a node: downcasting

To optimise, the optimizer sometimes has to ask "is this node a comparison? with which operator? are both children column references?" A trait object hides the concrete type, so the trait offers `as_any(&self) -> &dyn Any`, and the caller writes `expr.as_any().downcast_ref::<ComparisonExpression>()`, which is `Some(&ComparisonExpression)` if that is what the node is. It is C++'s `dynamic_cast`, with the same trade-off: a closed set of node types would be better served by an `enum` and a `match`, but an open set that other code can extend uses trait objects and pays for downcasts.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `class AbstractExpression { virtual Value Evaluate(...) const = 0; }` | `trait Expression { fn evaluate(&self, ...) -> Result<Value>; }` |
| `std::shared_ptr<AbstractExpression>` | `Arc<dyn Expression>` |
| `dynamic_cast<const Comparison *>(e.get())` (null if not) | `e.as_any().downcast_ref::<Comparison>()` (`None` if not) |
| `CloneWithChildren` copies the node then swaps the children vector | `clone_with_children(&self, Vec<ExprRef>) -> ExprRef` |

## In real code

### Using it: a small expression tree

```rust test
use std::any::Any;
use std::sync::Arc;

type E = Arc<dyn Expr>;

trait Expr: Send + Sync {
    fn eval(&self, row: &[i64]) -> i64;
    fn children(&self) -> &[E];
    fn with_children(&self, children: Vec<E>) -> E;
    fn as_any(&self) -> &dyn Any;
}

struct Column(usize);
struct Const(i64);
#[derive(Clone)]
struct Plus(Vec<E>);

impl Expr for Column {
    fn eval(&self, row: &[i64]) -> i64 { row[self.0] }
    fn children(&self) -> &[E] { &[] }
    fn with_children(&self, _: Vec<E>) -> E { Arc::new(Column(self.0)) }
    fn as_any(&self) -> &dyn Any { self }
}
impl Expr for Const {
    fn eval(&self, _: &[i64]) -> i64 { self.0 }
    fn children(&self) -> &[E] { &[] }
    fn with_children(&self, _: Vec<E>) -> E { Arc::new(Const(self.0)) }
    fn as_any(&self) -> &dyn Any { self }
}
impl Expr for Plus {
    fn eval(&self, row: &[i64]) -> i64 { self.0[0].eval(row) + self.0[1].eval(row) }
    fn children(&self) -> &[E] { &self.0 }
    fn with_children(&self, children: Vec<E>) -> E { Arc::new(Plus(children)) }
    fn as_any(&self) -> &dyn Any { self }
}

/// Rebuild the tree with every Column(i) shifted to Column(i + by): the kind of rewrite an optimizer makes.
fn shift_columns(e: &E, by: usize) -> E {
    let children: Vec<E> = e.children().iter().map(|c| shift_columns(c, by)).collect();
    match e.as_any().downcast_ref::<Column>() {
        Some(c) => Arc::new(Column(c.0 + by)),
        None => e.with_children(children),
    }
}

#[test]
fn a_tree_evaluates_by_asking_its_children() {
    // (#0 + 100) + #1
    let tree: E = Arc::new(Plus(vec![Arc::new(Plus(vec![Arc::new(Column(0)), Arc::new(Const(100))])), Arc::new(Column(1))]));
    assert_eq!(tree.eval(&[1, 20]), 121);
    assert_eq!(tree.eval(&[-100, 0]), 0);
}

#[test]
fn rewriting_builds_a_new_tree_and_leaves_the_old_one_alone() {
    let tree: E = Arc::new(Plus(vec![Arc::new(Column(0)), Arc::new(Const(5))]));
    let shifted = shift_columns(&tree, 2);
    assert_eq!(tree.eval(&[1, 2, 3]), 6, "the original still reads column 0");
    assert_eq!(shifted.eval(&[1, 2, 3]), 8, "the copy reads column 2");
    assert!(!Arc::ptr_eq(&tree, &shifted));
}

#[test]
fn a_downcast_tells_node_types_apart() {
    let leaf: E = Arc::new(Column(3));
    let sum: E = Arc::new(Plus(vec![leaf.clone(), Arc::new(Const(1))]));
    assert!(leaf.as_any().downcast_ref::<Column>().is_some());
    assert!(leaf.as_any().downcast_ref::<Plus>().is_none());
    assert!(sum.as_any().downcast_ref::<Plus>().is_some());
    assert_eq!(sum.children().len(), 2);
    assert_eq!(Arc::strong_count(&leaf), 2, "the leaf is shared with the tree, not copied");
}
```

### In the exercises

- **3d-01:** `ConstantValueExpression` and `ColumnValueExpression` are the two leaves; `evaluate_join` adds the second input; comparison and arithmetic nodes evaluate their children and combine them, exactly like `Plus` above.
- **3d-02:** the logic and string nodes do the same, and the factory you write adds the `lower`/`upper` nodes.
- **3d-04:** the parser you write builds the tree of the *syntax* (`Expr`); the planner (given) turns it into these nodes.
- **Module 3h (optimizer):** `rewrite_expression_for_join` is `shift_columns` with a twist; `as_any().downcast_ref` is how rules recognise patterns.

### Where it is used

- **PostgreSQL**: `ExprState` trees compiled to a flat list of steps for speed; the planner's `Expr` nodes are the equivalent of these.
- **DataFusion** (Rust): `PhysicalExpr` is a trait with `evaluate(&RecordBatch)`, children as `Arc<dyn PhysicalExpr>`, and `as_any` for downcasting, the same design.
- **SQLite**: compiles expressions to a bytecode program instead of a tree (the VDBE).
- **Interpreters of every kind**: an AST evaluated by recursive descent; *Crafting Interpreters* builds exactly this.
