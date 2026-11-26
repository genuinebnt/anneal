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

#[test]
fn roles_include_roles_through_any_number_of_links() {
    let g = graph(4, &[(0, 1, When::Always), (1, 2, When::Always)]);
    check!("0 includes 1 includes 2", got(&g, &[0], NOTHING), (vec![0, 1, 2], vec![]));
}

#[test]
fn nothing_held_means_nothing() {
    let g = graph(2, &[(0, 1, When::Mfa)]);
    check!("no held roles", got(&g, &[], MFA), (vec![], vec![]));
}

#[test]
fn a_closed_link_blocks_its_target() {
    let g = graph(4, &[(0, 1, When::Mfa), (1, 2, When::Always), (0, 3, When::OnSite)]);
    check!("no second factor, not on site", got(&g, &[0], NOTHING), (vec![0], vec![1, 3]));
    check!("with a second factor", got(&g, &[0], MFA), (vec![0, 1, 2], vec![3]));
    check!("on site", got(&g, &[0], SITE), (vec![0, 3], vec![1]));
}

#[test]
fn loops_end_and_repeats_are_listed_once() {
    let g = graph(3, &[(0, 1, When::Always), (1, 0, When::Always), (1, 1, When::Always), (1, 2, When::Mfa), (0, 2, When::Mfa)]);
    check!("a loop, and 2 blocked twice", got(&g, &[1, 1, 0], NOTHING), (vec![0, 1], vec![2]));
}

#[test]
fn a_role_reached_another_way_is_not_blocked() {
    let g = graph(3, &[(0, 1, When::Mfa), (0, 2, When::Always), (2, 1, When::Always)]);
    check!("1 is reached through 2", got(&g, &[0], NOTHING), (vec![0, 1, 2], vec![]));
}
