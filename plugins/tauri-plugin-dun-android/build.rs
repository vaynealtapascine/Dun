// No commands are exposed to JavaScript; the app's Rust side calls into the
// Kotlin plugin directly with `run_mobile_plugin`.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
