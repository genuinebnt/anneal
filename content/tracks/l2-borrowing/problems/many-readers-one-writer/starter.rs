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

// TODO: impl Inbox.

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
