// Hide the console window on Windows release builds — a toolbar that spawns a
// black terminal behind it is not a toolbar anyone wants.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    token_bar_lib::run()
}
