//! Run-length encoding of bytes: `aaab` becomes (3, a) (1, b). A building block of real compression formats.

/// Why a byte string is not run-length encoded data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RleError {
    /// A count without its byte.
    Truncated,
    /// A run of length 0 means nothing.
    ZeroRun,
}

/// Each run of equal bytes becomes a pair: the length of the run (1 to 255), then the byte. Longer runs become several pairs.
pub fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut run = 1usize;
        while i + run < data.len() && data[i + run] == byte {
            run += 1;
        }
        // @begin r-c2
        let mut left = run;
        while left > 0 {
            let n = left.min(255);
            out.push(n as u8);
            out.push(byte);
            left -= n;
        }
        //~ // TODO(r-c2): this looks right and has a bug; find it and fix it
        //~ out.push(run as u8);
        //~ out.push(byte);
        // @end
        i += run;
    }
    out
}

/// The inverse of `rle_encode`.
pub fn rle_decode(bytes: &[u8]) -> Result<Vec<u8>, RleError> {
    let mut out = Vec::new();
    for pair in bytes.chunks(2) {
        let [n, byte] = pair else { return Err(RleError::Truncated) };
        if *n == 0 {
            return Err(RleError::ZeroRun);
        }
        out.extend(std::iter::repeat_n(*byte, *n as usize));
    }
    Ok(out)
}
