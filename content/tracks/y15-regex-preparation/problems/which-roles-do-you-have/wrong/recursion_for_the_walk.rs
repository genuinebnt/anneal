#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Always,
    Mfa,
    OnSite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub to: usize,
    pub when: When,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub mfa: bool,
    pub on_site: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub roles: Vec<usize>,
    pub blocked: Vec<usize>,
}

fn visit(links: &[Vec<Link>], role: usize, context: Context, has: &mut [bool], closed: &mut [bool]) {
    if std::mem::replace(&mut has[role], true) {
        return;
    }
    for link in &links[role] {
        let open = match link.when {
            When::Always => true,
            When::Mfa => context.mfa,
            When::OnSite => context.on_site,
        };
        if !open {
            closed[link.to] = true;
        } else {
            visit(links, link.to, context, has, closed);
        }
    }
}

pub fn access(links: &[Vec<Link>], held: &[usize], context: Context) -> Access {
    let mut has = vec![false; links.len()];
    let mut closed = vec![false; links.len()];
    for &role in held {
        visit(links, role, context, &mut has, &mut closed);
    }
    Access {
        roles: (0..links.len()).filter(|&r| has[r]).collect(),
        blocked: (0..links.len()).filter(|&r| closed[r] && !has[r]).collect(),
    }
}
