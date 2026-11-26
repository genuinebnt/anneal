//! Authorization as one pure function: (who, what, whose) -> allow or deny.
//!
//! WHY one function: scattered `if role == "admin"` checks drift apart and cannot be audited. Here the
//! whole matrix is visible and testable without a database or HTTP.
//! Tenant scoping is deliberately NOT here: every repository query filters by `org_id`, so another
//! tenant's object is simply "not found" (404, which does not confirm that it exists). This function only
//! ever sees objects already known to belong to the caller's org.
//! Production alternatives: `casbin`, Oso, AWS Cedar, OpenFGA (relationship-based, Zanzibar style).
//! Recipe: `examples/web/backend_authz_rbac_abac_policies.rs`.

use crate::error::{AppError, AppResult};

/// Ordered from least to most powerful: `>=` comparisons rely on this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "viewer" => Self::Viewer,
            "member" => Self::Member,
            "admin" => Self::Admin,
            "owner" => Self::Owner,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }
}

/// How the caller authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// Interactive login (access token). Carries the session id.
    Session(String),
    /// Long-lived machine credential.
    ApiKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub user_id: String,
    pub org_id: String,
    pub role: Role,
    pub via: Via,
}

#[derive(Debug, Clone, Copy)]
pub enum Action<'a> {
    Read,
    /// Create a project, task or comment, or upload an attachment.
    Create,
    /// Change a project or task. `owners` are the users with a personal stake: creator and assignee.
    Modify {
        owners: &'a [&'a str],
    },
    /// Delete a project or task.
    Delete {
        creator: &'a str,
    },
    ManageWebhooks,
    /// Add a user with the given role.
    AddMember(Role),
    ReadAudit,
    ManageApiKeys,
}

impl Principal {
    #[must_use]
    pub fn can(&self, action: Action<'_>) -> bool {
        let me = self.user_id.as_str();
        match action {
            Action::Read => true,
            Action::Create => self.role >= Role::Member,
            // A demoted user keeps ownership of old rows but loses write access: check the role first.
            Action::Modify { owners } => {
                self.role >= Role::Admin || (self.role == Role::Member && owners.contains(&me))
            }
            Action::Delete { creator } => self.role >= Role::Admin || (self.role == Role::Member && creator == me),
            Action::ManageWebhooks | Action::ReadAudit => self.role >= Role::Admin,
            // Nobody can mint a peer or superior, except owners: otherwise one admin can create more
            // admins and the owner loses control.
            Action::AddMember(new) => self.role == Role::Owner || (self.role == Role::Admin && new < Role::Admin),
            // A stolen API key must not be able to mint more keys and outlive its revocation.
            Action::ManageApiKeys => matches!(self.via, Via::Session(_)),
        }
    }

    pub fn require(&self, action: Action<'_>) -> AppResult<()> {
        if self.can(action) { Ok(()) } else { Err(AppError::Forbidden) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Role::{Admin, Member, Owner, Viewer};

    fn who(role: Role) -> Principal {
        Principal { user_id: "me".into(), org_id: "o".into(), role, via: Via::Session("s".into()) }
    }

    /// The matrix written out by hand: the test is the specification, not a copy of the `match`.
    #[test]
    fn full_matrix() {
        let mine: &[&str] = &["me"];
        let theirs: &[&str] = &["you"];
        let rows: Vec<(Action<'_>, [bool; 4])> = vec![
            // action, [viewer, member, admin, owner]
            (Action::Read, [true, true, true, true]),
            (Action::Create, [false, true, true, true]),
            (Action::Modify { owners: mine }, [false, true, true, true]),
            (Action::Modify { owners: theirs }, [false, false, true, true]),
            (Action::Delete { creator: "me" }, [false, true, true, true]),
            (Action::Delete { creator: "you" }, [false, false, true, true]),
            (Action::ManageWebhooks, [false, false, true, true]),
            (Action::ReadAudit, [false, false, true, true]),
            (Action::AddMember(Viewer), [false, false, true, true]),
            (Action::AddMember(Member), [false, false, true, true]),
            (Action::AddMember(Admin), [false, false, false, true]),
            (Action::AddMember(Owner), [false, false, false, true]),
            (Action::ManageApiKeys, [true, true, true, true]),
        ];
        for (action, expected) in rows {
            for (role, want) in [Viewer, Member, Admin, Owner].into_iter().zip(expected) {
                assert_eq!(who(role).can(action), want, "{role:?} {action:?}");
            }
        }
    }

    #[test]
    fn api_keys_cannot_manage_api_keys_even_for_owners() {
        let mut p = who(Owner);
        p.via = Via::ApiKey("k".into());
        assert!(!p.can(Action::ManageApiKeys));
    }

    #[test]
    fn role_strings_round_trip() {
        for r in [Viewer, Member, Admin, Owner] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("root"), None);
    }
}
