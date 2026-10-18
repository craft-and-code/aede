//! Native server application entry point.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() -> Result<(), tauri::Error> {
    aede_desktop_lib::run()
}
