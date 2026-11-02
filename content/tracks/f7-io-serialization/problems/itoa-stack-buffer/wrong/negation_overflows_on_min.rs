pub fn fmt_u64(n: u64, buf: &mut [u8; 20]) -> &str {
    let mut n = n;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8; // fill from the right: the most significant digit ends up first
        n /= 10;
        if n == 0 {
            break; // a do-while: 0 still prints one digit
        }
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}

pub fn fmt_i64(n: i64, buf: &mut [u8; 20]) -> &str {
    let neg = n < 0;
    let mut m = (if neg { -n } else { n }) as u64;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}

pub fn fmt_grouped(n: u64, buf: &mut [u8; 26]) -> &str {
    let mut n = n;
    let mut i = buf.len();
    let mut digits = 0;
    loop {
        if digits > 0 && digits % 3 == 0 {
            i -= 1;
            buf[i] = b',';
        }
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        digits += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}
