use cow_borrow_until_you_must_own::escape_html;
use std::borrow::Cow;

fn main() {
    for input in ["plain text", "a < b && c", "say \"hi\""] {
        let out = escape_html(input);
        let kind = match out {
            Cow::Borrowed(_) => "borrowed",
            Cow::Owned(_) => "owned",
        };
        println!("{input:<12} -> {out:<28} {kind}");
    }
    println!(
        "size in bytes: &str {}, String {}, Cow<str> {}",
        size_of::<&str>(),
        size_of::<String>(),
        size_of::<Cow<'static, str>>()
    );
}
