use cb_macros::memoize;
struct S;
impl S {
    #[memoize]
    fn f(&self) -> u8 {
        1
    }
}
fn main() {}
