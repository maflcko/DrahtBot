// # Short Description
//
// Requires:
// cargo clean --package=inverse_fdp && cargo test && cargo run
//
// But first, insert the generated code below the comment `// Insert below` in the main function
//
// # Long Description
//
// The general idea here is that with a FuzzedDataProvider-based fuzz target, each fuzz input may
// have a different format. Unlike protobuf-based (or similar) fuzz targets.
//
// This leads to the havoc effect, where a minimal change in the fuzz input bytes may trigger a
// completely different execution.
//
// This project is mostly for fun, to make it easier to expand/mutate properly formatted fuzz
// inputs by hand. For example, to swap one byte blob with another, or swap one integral enum value
// with another.
//
// The approach is:
//
// * Compile the fuzz target with the FuzzedDataProvider.example.patch
// * Run the fuzz target with the fuzz input you want to know the format of
// * Copy-paste the format printed to stdout into the main function `// Insert below`
// * Modify the raw bytes, the format, or the values to your liking
// * Run the main function `cargo clean --package=inverse_fdp && cargo test && cargo run` to
//   generate the new fuzz input
// * Pass the new fuzz input to the fuzz target and see what happens (fun)
//
extern crate cpp_build;
fn main() {
    cpp_build::build("src/lib.rs");
}
