use std::marker::PhantomData;

/// 3 to 16 characters: a lowercase ASCII letter, then lowercase letters, digits or `_`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

pub struct LoggedOut;
pub struct LoggedIn;
pub struct Locked;

pub struct User<S> {
    name: Username,
    password: String,
    failures: u32,
    state: PhantomData<S>,
}

/// A failed login hands the user back, in whichever state it ended up.
pub enum LoginError {
    WrongPassword(User<LoggedOut>),
    Locked(User<Locked>),
}

impl Username {
    pub fn parse(s: &str) -> Option<Username> {
        let mut chars = s.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let ok = first_ok
            && (3..=16).contains(&s.len())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        ok.then(|| Username(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<S> User<S> {
    pub fn name(&self) -> &Username {
        &self.name
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    // Changing the state parameter builds a new value; moving the fields keeps it free.
    fn into_state<T>(self) -> User<T> {
        User {
            name: self.name,
            password: self.password,
            failures: self.failures,
            state: PhantomData,
        }
    }
}

impl User<LoggedOut> {
    pub fn new(name: Username, password: &str) -> Self {
        User {
            name,
            password: password.to_string(),
            failures: 0,
            state: PhantomData,
        }
    }

    /// Right password: logged in, failures reset. Wrong: one more failure; the third in a row locks the account.
    pub fn login(mut self, password: &str) -> Result<User<LoggedIn>, LoginError> {
        if password == self.password {
            self.failures = 0;
            return Ok(self.into_state());
        }
        self.failures += 1;
        if self.failures >= 3 {
            Err(LoginError::Locked(self.into_state()))
        } else {
            Err(LoginError::WrongPassword(self))
        }
    }
}

impl User<LoggedIn> {
    pub fn dashboard(&self) -> String {
        format!("dashboard:{}", self.name.as_str())
    }

    pub fn change_password(&mut self, new: &str) {
        self.password = new.to_string();
    }

    pub fn logout(self) -> User<LoggedOut> {
        self.into_state()
    }
}

impl User<Locked> {
    /// An admin unlock: back to logged out with no failures.
    pub fn unlock(mut self) -> User<LoggedOut> {
        self.failures = 0;
        self.into_state()
    }
}
