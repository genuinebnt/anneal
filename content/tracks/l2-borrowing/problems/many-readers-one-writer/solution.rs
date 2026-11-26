#[derive(Debug, PartialEq)]
pub struct Msg {
    pub id: u32,
    pub from: String,
    pub body: String,
    pub read: bool,
    pub reply: Option<String>,
}

/// Messages in arrival order, oldest first. Ids are unique.
pub struct Inbox {
    msgs: Vec<Msg>,
}

impl Inbox {
    pub fn new() -> Self {
        Inbox { msgs: Vec::new() }
    }

    pub fn push(&mut self, id: u32, from: &str, body: &str) {
        self.msgs.push(Msg {
            id,
            from: from.to_string(),
            body: body.to_string(),
            read: false,
            reply: None,
        });
    }

    pub fn get(&self, id: u32) -> Option<&Msg> {
        self.msgs.iter().find(|m| m.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Msg> {
        self.msgs.iter_mut().find(|m| m.id == id)
    }

    pub fn unread(&self) -> Vec<&Msg> {
        self.msgs.iter().filter(|m| !m.read).collect()
    }

    pub fn reply_to(&self, id: u32) -> Option<&str> {
        self.get(id)?.reply.as_deref()
    }

    pub fn mark_read(&mut self, id: u32) -> bool {
        match self.get_mut(id) {
            Some(m) if !m.read => {
                m.read = true;
                true
            }
            _ => false,
        }
    }

    pub fn reply_mut(&mut self, id: u32) -> Option<&mut String> {
        Some(self.get_mut(id)?.reply.get_or_insert_with(String::new))
    }

    pub fn newest_unread_mut(&mut self) -> Option<&mut Msg> {
        self.msgs.iter_mut().rev().find(|m| !m.read)
    }
}

/// Marks every unread message from `boss` read, appends "on it" to the reply of the newest message still
/// unread (if there is one), and returns the ids still unread, oldest first.
pub fn triage(inbox: &mut Inbox, boss: &str) -> Vec<u32> {
    let from_boss: Vec<u32> = inbox
        .unread()
        .iter()
        .filter(|m| m.from == boss)
        .map(|m| m.id)
        .collect();
    for id in from_boss {
        inbox.mark_read(id);
    }
    if let Some(m) = inbox.newest_unread_mut() {
        m.reply.get_or_insert_with(String::new).push_str("on it");
    }
    inbox.unread().iter().map(|m| m.id).collect()
}
