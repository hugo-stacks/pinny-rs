use trybuild::TestCases;

#[test]
fn test_failures() {
    // setting `PINNY_CARGO_MANIFEST_DIR` as on override for `CARGO_MANIFEST_DIR` to execute tests with trybuilder crate
    // This workaroubnd is suggested even here https://github.com/dtolnay/trybuild/issues/202
    // SAFETY: this is the only test in this binary, so no other thread
    // touches the environment concurrently.
    unsafe {
        std::env::set_var(
            "PINNY_CARGO_MANIFEST_DIR",
            std::env::var("CARGO_MANIFEST_DIR").unwrap(),
        );
    }

    let case = TestCases::new();
    case.compile_fail("tests/failures/*.rs");
}
