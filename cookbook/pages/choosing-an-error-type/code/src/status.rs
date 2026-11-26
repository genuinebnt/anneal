//! Turning an error into what the outside world sees. Do this in one place, at the edge.

use crate::derived::PortError;
use std::io::ErrorKind;

/// For a command-line program: 2 for input the user can fix, 1 for anything else.
pub fn exit_code(error: &PortError) -> u8 {
    match error {
        PortError::Missing | PortError::NotANumber { .. } | PortError::Zero => 2,
        PortError::Read { source, .. } if source.kind() == ErrorKind::NotFound => 2,
        PortError::Read { .. } => 1,
    }
}

/// For a service where the client names the file.
pub fn http_status(error: &PortError) -> u16 {
    match error {
        PortError::Read { source, .. } if source.kind() == ErrorKind::NotFound => 404,
        PortError::Read { .. } => 500,
        PortError::Missing | PortError::NotANumber { .. } | PortError::Zero => 422,
    }
}

/// What a client may read. Details of a 500 belong in the log.
pub fn public_message(error: &PortError) -> String {
    match http_status(error) {
        500 => "internal error".to_owned(),
        _ => error.to_string(),
    }
}

/// With a type-erased error, the status depends on a downcast that can miss.
/// Anything unrecognised must fall back to 500.
pub fn http_status_of(error: &anyhow::Error) -> u16 {
    error.downcast_ref::<PortError>().map_or(500, http_status)
}
