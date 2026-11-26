//! RECIPE 76 (patterns): typestate protocol: a session whose legal operations depend on its state, enforced by the compiler.
//! LEVEL: advanced
//! Use in: connections, builders with required steps, lifecycles where misuse must be impossible (not just an `Err`).   Run: cargo run --release --example typestate_protocol
//! COMMON USES: protocol clients, builders with mandatory steps, hardware drivers; invalid sequences fail to compile.
#![allow(dead_code)]
//!
//! Pattern: `Session<S>` is generic over a state type. Each state is its own (often zero-sized) type, and methods live in
//! `impl Session<ThatState>` blocks, so `send()` simply DOES NOT EXIST on a logged-out session. Transitions take `self`
//! BY VALUE and return the next state type, so the old state cannot be reused. A SEALED trait stops outside code from
//! inventing new states. Failed transitions can hand the old session BACK in the `Err` so callers can retry.
//! Compare with the enum state machine (state_machine): enums decide at RUNTIME and can be stored in one variable; typestate decides
//! at COMPILE TIME but each state is a different type (awkward in collections). Pick by where mistakes must be caught.
//! Read: https://cliffle.com/blog/rust-typestate/ and https://rust-unofficial.github.io/patterns/
mod sealed {
    pub trait Sealed {}
}
pub trait State: sealed::Sealed {
    const NAME: &'static str;
}

pub struct Disconnected;
pub struct Connected {
    server: String,
}
pub struct LoggedIn {
    server: String,
    user: String,
}
macro_rules! state {
    ($t:ty, $n:expr) => {
        impl sealed::Sealed for $t {}
        impl State for $t {
            const NAME: &'static str = $n;
        }
    };
}
state!(Disconnected, "disconnected");
state!(Connected, "connected");
state!(LoggedIn, "logged-in");

pub struct Session<S: State> {
    state: S,
    log: Vec<String>,
}

#[derive(Debug)]
pub struct AuthError(&'static str);

// methods available in EVERY state
impl<S: State> Session<S> {
    fn note(&mut self, m: String) {
        self.log.push(format!("[{}] {m}", S::NAME));
    }
    pub fn history(&self) -> &[String] {
        &self.log
    }
    pub fn state_name(&self) -> &'static str {
        S::NAME
    }
}

impl Session<Disconnected> {
    pub fn new() -> Self {
        Session {
            state: Disconnected,
            log: vec![],
        }
    }
    pub fn connect(mut self, server: &str) -> Session<Connected> {
        self.note(format!("connecting to {server}"));
        Session {
            state: Connected {
                server: server.into(),
            },
            log: self.log,
        }
    }
}

impl Session<Connected> {
    /// On failure the SAME session comes back so the caller can try again (no state is lost).
    pub fn login(
        mut self,
        user: &str,
        password: &str,
    ) -> Result<Session<LoggedIn>, (Session<Connected>, AuthError)> {
        if password != "hunter2" {
            self.note(format!("login failed for {user}"));
            return Err((self, AuthError("bad password")));
        }
        self.note(format!("{user} logged in"));
        Ok(Session {
            state: LoggedIn {
                server: self.state.server,
                user: user.into(),
            },
            log: self.log,
        })
    }
    pub fn disconnect(mut self) -> Session<Disconnected> {
        self.note("bye".into());
        Session {
            state: Disconnected,
            log: self.log,
        }
    }
}

impl Session<LoggedIn> {
    pub fn send(&mut self, msg: &str) {
        let line = format!("{} -> {}: {msg}", self.state.user, self.state.server);
        self.note(line);
    }
    pub fn logout(mut self) -> Session<Connected> {
        self.note("logout".into());
        let LoggedIn { server, .. } = self.state;
        Session {
            state: Connected { server },
            log: self.log,
        }
    }
}

fn main() {
    let s = Session::new().connect("example.org");
    let s = match s.login("ana", "wrong") {
        Ok(_) => unreachable!(),
        Err((s, e)) => {
            println!("login error: {e:?}; still {}", s.state_name());
            s // got the session back
        }
    };
    let mut s = s.login("ana", "hunter2").ok().expect("correct password");
    s.send("hello");
    let s = s.logout().disconnect();
    for line in s.history() {
        println!("{line}");
    }
    println!(
        "final state: {} (state types are zero-cost: Disconnected is {} bytes)",
        s.state_name(),
        std::mem::size_of::<Disconnected>()
    );

    // These do NOT compile (try them):
    // Session::new().send("x");                  // no method `send` on Session<Disconnected>
    // let c = Session::new().connect("h"); c.login("a","b"); c.login("a","b");   // use of moved value: login consumed `c`
    // impl State for String {}                   // can't: `Sealed` is private to this module
}
