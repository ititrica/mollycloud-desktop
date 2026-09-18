// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if molly_skills::is_cli_request() {
        molly_skills::cli::main();
        return;
    }
    mollycloud_lib::run()
}
