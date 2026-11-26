//! Two things every error consumer needs: print the whole chain, and find a cause by type.

use std::error::Error;
use std::iter::successors;

/// The error, then its source, then that error's source, and so on.
pub fn sources<'a>(
    error: &'a (dyn Error + 'static),
) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    successors(Some(error), |&e| e.source())
}

/// `outer: middle: root`
pub fn one_line(error: &(dyn Error + 'static)) -> String {
    sources(error)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

/// The first error of type `T` anywhere in the chain.
pub fn find<'a, T: Error + 'static>(error: &'a (dyn Error + 'static)) -> Option<&'a T> {
    sources(error).find_map(|e| e.downcast_ref::<T>())
}
