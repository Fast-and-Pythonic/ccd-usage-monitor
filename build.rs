//! Compiles the settings window into the binary.
//!
//! Only the settings window. `themes/pill.slint` is deliberately *not* here:
//! it is read at runtime so that it can be edited without a Rust toolchain,
//! which is the whole point of A20. A form is a different kind of thing — its
//! appearance is not a setting, and it needs `std-widgets`.

fn main() {
   slint_build::compile("ui/settings.slint").expect("the settings window must compile");
}
