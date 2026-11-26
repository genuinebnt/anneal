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

pub fn access(links: &[Vec<Link>], held: &[usize], context: Context) -> Access {
    let mut has = vec![false; links.len()];
    let mut closed = vec![false; links.len()];
    let mut work: Vec<usize> = held.to_vec();
    while let Some(role) = work.pop() {
        if std::mem::replace(&mut has[role], true) {
            continue;
        }
        for link in &links[role] {
            let open = match link.when {
                When::Always => true,
                When::Mfa => context.mfa,
                When::OnSite => context.on_site || context.mfa,
            };
            if !open {
                closed[link.to] = true;
            } else if !has[link.to] {
                work.push(link.to);
            }
        }
    }
    Access {
        roles: (0..links.len()).filter(|&r| has[r]).collect(),
        blocked: (0..links.len()).filter(|&r| closed[r] && !has[r]).collect(),
    }
}
