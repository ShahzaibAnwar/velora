// Release builds use the GUI subsystem (no console window). Inherited stdio handles still work, which
// is what native-messaging host mode relies on.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // A browser launches Velora as its native-messaging host with the extension origin as an argument.
    // Handle that before any window or runtime is created.
    if velora_lib::bridge::host::is_native_host_invocation(&args) {
        std::process::exit(velora_lib::bridge::host::run());
    }
    velora_lib::run();
}
