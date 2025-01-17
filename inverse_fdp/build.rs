// Requires:
// cargo clean --package=inverse_fdp && cargo test
extern crate cpp_build;
fn main() {
    cpp_build::build("src/main.rs");
}
