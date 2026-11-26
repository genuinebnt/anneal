use choosing_an_error_type::{app, boxed, derived, hand, report, status};
use std::path::Path;

fn main() {
    let missing = Path::new("/definitely/missing.conf");

    println!("== the same missing file, four ways ==");
    if let Err(e) = hand::load_port(missing) {
        println!("hand      {}", report::one_line(&e));
    }
    if let Err(e) = derived::load_port(missing) {
        println!("thiserror {}", report::one_line(&e));
        println!(
            "          exit code {}, HTTP {}",
            status::exit_code(&e),
            status::http_status(&e)
        );
    }
    if let Err(e) = app::load_port(missing) {
        println!("anyhow    {e:#}");
    }
    if let Err(e) = boxed::load_port(missing) {
        println!("boxed     {}", report::one_line(e.as_ref()));
    }

    println!("\n== one anyhow error, four formats ==");
    if let Err(e) = app::parse_port("port=eighty") {
        println!("{{}}    {e}");
        println!("{{:#}}  {e:#}");
        println!("{{:?}}\n{e:?}");
        println!("{{:#?}}\n{e:#?}");
    }

    println!("\n== what `fn main() -> Result` prints: Debug ==");
    if let Err(e) = derived::parse_port("port=0") {
        println!("thiserror enum  Error: {e:?}");
    }
    if let Err(e) = boxed::parse_port("port=0") {
        println!("boxed           Error: {e:?}");
    }
}
