// ===================================================================== the protocol (step 1)

pub type Args = Vec<Vec<u8>>;

/// One value of the protocol.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Frame>),
    NullArray,
}

/// What the reader accepts. Every length in the input is untrusted.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_line: usize,
    pub max_bulk: usize,
    pub max_array: usize,
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_line: 64 * 1024,
            max_bulk: 1 << 20,
            max_array: 1 << 16,
            max_depth: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoError {
    /// The first byte of a frame is not one of `+ - : $ *`.
    BadType(u8),
    BadInteger,
    BadLength,
    BadTerminator,
    BadUtf8,
    LineTooLong,
    TooLarge,
    TooDeep,
    /// An element of a command is not a bulk string; the byte is its type byte.
    ExpectedBulk(u8),
}

/// One frame from the front of `buf`: the frame and the bytes it takes, or `Ok(None)` for "need more".
pub fn parse_frame(buf: &[u8], limits: &Limits) -> Result<Option<(Frame, usize)>, ProtoError> {
    todo!()
}

/// One client command from the front of `buf`: its arguments and the bytes it takes.
pub fn parse_command(buf: &[u8], limits: &Limits) -> Result<Option<(Args, usize)>, ProtoError> {
    todo!()
}

/// Appends the bytes of `frame` to `out`.
pub fn encode(frame: &Frame, out: &mut Vec<u8>) {
    todo!()
}

/// A command as a client sends it: an array of bulk strings.
pub fn encode_command(args: &[Vec<u8>]) -> Vec<u8> {
    todo!()
}
