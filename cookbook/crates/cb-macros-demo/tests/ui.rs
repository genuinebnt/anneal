// trybuild compiles each file in tests/ui and compares rustc's stderr with the
// `.stderr` snapshot next to it. Regenerate with `TRYBUILD=overwrite`.
#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
