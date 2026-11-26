//! RECIPE 67 (graphs): strongly connected components (iterative Tarjan): find circular groups in a directed graph.
//! LEVEL: advanced
//! Use in: diagnosing WHICH nodes form cycles (not just "a cycle exists"), condensing a graph into a DAG.   Run: cargo run --release --example tarjan_scc
//! COMMON USES: finding circular dependencies (build systems, module imports, Cargo features), condensing graphs to DAGs.
#![allow(dead_code)]
//!
//! An SCC is a maximal set of nodes that can all reach each other. Collapsing each SCC to one node always yields a DAG.
//! Components of size > 1 (or a node with a self-edge) are the cycles. Tarjan does it in ONE DFS, O(V+E), using
//! `index` (discovery order) and `low` (lowest index reachable). Here the DFS is ITERATIVE (explicit call stack) so deep
//! graphs cannot overflow the stack (see stage 8). Components come out in reverse topological order of the condensation.
//! Domain: module imports. Read: https://en.wikipedia.org/wiki/Tarjan%27s_strongly_connected_components_algorithm
//! and petgraph's `tarjan_scc` https://docs.rs/petgraph/latest/petgraph/algo/fn.tarjan_scc.html
fn tarjan(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let n = adj.len();
    let (mut index, mut low) = (vec![UNSEEN; n], vec![0; n]);
    let mut on_stack = vec![false; n];
    let (mut stack, mut comps) = (vec![], vec![]);
    let mut next_index = 0;

    for root in 0..n {
        if index[root] != UNSEEN {
            continue;
        }
        // frame = (node, position of the next outgoing edge to look at)
        let mut call: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;

        while let Some(&(v, ei)) = call.last() {
            if ei < adj[v].len() {
                call.last_mut().unwrap().1 += 1;
                let w = adj[v][ei];
                if index[w] == UNSEEN {
                    index[w] = next_index;
                    low[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    call.push((w, 0)); // "recurse"
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                if low[v] == index[v] {
                    // v is the root of a component: pop it off
                    let mut comp = vec![];
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    comp.sort();
                    comps.push(comp);
                }
                call.pop(); // "return"
                if let Some(&(parent, _)) = call.last() {
                    low[parent] = low[parent].min(low[v]);
                }
            }
        }
    }
    comps
}

fn main() {
    let names = ["app", "net", "db", "log", "util", "cfg", "tool"];
    // edges = "imports"
    let edges = [
        (0, 1),
        (1, 2),
        (2, 0),
        (2, 3),
        (3, 4),
        (4, 3),
        (5, 0),
        (6, 6),
    ];
    let mut adj = vec![vec![]; names.len()];
    for (a, b) in edges {
        adj[a].push(b);
    }
    let comps = tarjan(&adj);
    for c in &comps {
        let list: Vec<_> = c.iter().map(|&i| names[i]).collect();
        let cyc = c.len() > 1 || adj[c[0]].contains(&c[0]);
        println!("{} {list:?}", if cyc { "CYCLE" } else { "     " });
    }
    // invariant: Tarjan emits components in reverse topological order
    let comp_of: Vec<usize> = {
        let mut m = vec![0; names.len()];
        for (ci, c) in comps.iter().enumerate() {
            c.iter().for_each(|&v| m[v] = ci);
        }
        m
    };
    for (a, b) in edges {
        assert!(comp_of[a] == comp_of[b] || comp_of[a] > comp_of[b]);
    }
    assert_eq!(comps.iter().filter(|c| c.len() > 1).count(), 2);

    let deep: Vec<Vec<usize>> = (0..200_000)
        .map(|i| if i + 1 < 200_000 { vec![i + 1] } else { vec![] })
        .collect();
    println!(
        "200k-deep chain: {} components, no stack overflow",
        tarjan(&deep).len()
    );
}
