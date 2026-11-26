use solution::*;

#[allow(dead_code)]
fn graph(n: usize, edges: &[(usize, usize, When)]) -> Vec<Vec<Link>> {
    let mut links = vec![Vec::new(); n];
    for &(from, to, when) in edges {
        links[from].push(Link { to, when });
    }
    links
}

#[allow(dead_code)]
const NOTHING: Context = Context { mfa: false, on_site: false };
#[allow(dead_code)]
const MFA: Context = Context { mfa: true, on_site: false };
#[allow(dead_code)]
const SITE: Context = Context { mfa: false, on_site: true };

#[allow(dead_code)]
fn got(links: &[Vec<Link>], held: &[usize], context: Context) -> (Vec<usize>, Vec<usize>) {
    let a = access(links, held, context);
    (a.roles, a.blocked)
}

use anneal_prelude::Rng;

#[test]
fn a_held_role_is_never_blocked() {
    let g = graph(2, &[(0, 1, When::Mfa)]);
    check!("1 is held directly and also behind a closed link", got(&g, &[1, 0], NOTHING), (vec![0, 1], vec![]));
}

#[test]
fn blocked_is_one_step_only() {
    let g = graph(4, &[(0, 1, When::Mfa), (1, 2, When::Mfa), (2, 3, When::Always)]);
    check!("only the first locked door is reported", got(&g, &[0], NOTHING), (vec![0], vec![1]));
}

#[test]
fn closed_links_from_roles_you_lack_do_not_count() {
    let g = graph(4, &[(2, 3, When::Mfa), (0, 1, When::Always)]);
    check!("role 2 is not held", got(&g, &[0], NOTHING), (vec![0, 1], vec![]));
}

#[test]
fn random_graphs_agree_with_repeating_until_nothing_changes() {
    let mut rng = Rng::new(1504);
    for _ in 0..800 {
        let n = rng.int(1, 8) as usize;
        let m = rng.below(16);
        let edges: Vec<(usize, usize, When)> = (0..m)
            .map(|_| {
                let from = rng.below(n);
                let to = rng.below(n);
                (from, to, *rng.pick(&[When::Always, When::Always, When::Mfa, When::OnSite]))
            })
            .collect();
        let k = rng.below(3);
        let held: Vec<usize> = (0..k).map(|_| rng.below(n)).collect();
        let context = Context { mfa: rng.bool(), on_site: rng.bool() };
        let open = |w: When| w == When::Always || (w == When::Mfa && context.mfa) || (w == When::OnSite && context.on_site);
        let mut has: Vec<bool> = (0..n).map(|r| held.contains(&r)).collect();
        for _ in 0..n {
            for &(from, to, w) in &edges {
                if has[from] && open(w) {
                    has[to] = true;
                }
            }
        }
        let roles: Vec<usize> = (0..n).filter(|&r| has[r]).collect();
        let blocked: Vec<usize> = (0..n).filter(|&r| !has[r] && edges.iter().any(|&(f, t, w)| t == r && has[f] && !open(w))).collect();
        check!(format!("{n} roles, links {edges:?}, held {held:?}, {context:?}"), got(&graph(n, &edges), &held, context), (roles, blocked));
    }
}

#[test]
fn a_very_long_chain() {
    let n = 500_000;
    let edges: Vec<(usize, usize, When)> = (0..n - 1).map(|i| (i, i + 1, When::Always)).collect();
    let a = access(&graph(n, &edges), &[0], NOTHING);
    check!("500 000 roles in a chain", (a.roles.len(), a.blocked.len()), (n, 0));
}

#[test]
fn more_paths_than_anyone_could_follow_one_by_one() {
    // 60 layers, two roles each, every role including both roles of the next layer.
    let mut edges = Vec::new();
    for layer in 0..59 {
        for a in 0..2 {
            for b in 0..2 {
                edges.push((layer * 2 + a, (layer + 1) * 2 + b, When::Always));
            }
        }
    }
    let a = access(&graph(120, &edges), &[0], NOTHING);
    check!("2^59 paths, 119 roles", a.roles.len(), 119);
}

#[test]
fn many_held_roles_that_share_a_long_tail() {
    let n = 100_000;
    let edges: Vec<(usize, usize, When)> = (0..n - 1).map(|i| (i, i + 1, When::Always)).collect();
    let held: Vec<usize> = (0..n).collect();
    let a = access(&graph(n, &edges), &held, NOTHING);
    check!("every role held, each with the same tail", a.roles.len(), n);
}

#[test]
fn both_conditions_at_once() {
    let g = graph(3, &[(0, 1, When::Mfa), (1, 2, When::OnSite)]);
    let both = Context { mfa: true, on_site: true };
    check!("with both facts true", got(&g, &[0], both), (vec![0, 1, 2], vec![]));
}
