// Release builds are GUI apps: no console window next to Blend Stack.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    blendstack_lib::run()
}
