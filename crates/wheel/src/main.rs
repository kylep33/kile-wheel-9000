use kile_wheel_core::names::DISPLAY_NAME;

fn main() {
    println!("{DISPLAY_NAME} {}", env!("CARGO_PKG_VERSION"));
}
