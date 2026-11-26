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
    todo!()
}
