#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub path: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

/// Everything inside the current step.
pub type Next<'a> = &'a dyn Fn(&Call) -> Answer;

pub struct Pipeline {
    // TODO
}

impl Pipeline {
    pub fn new(handler: impl Fn(&Call) -> Answer + Send + Sync + 'static) -> Pipeline {
        todo!()
    }

    pub fn wrap(
        self,
        step: impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static,
    ) -> Pipeline {
        todo!()
    }

    pub fn run(&self, call: &Call) -> Answer {
        todo!()
    }
}

pub fn shield(
    report: impl Fn(&str) + Send + Sync + 'static,
) -> impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static {
    |call, next| todo!()
}
