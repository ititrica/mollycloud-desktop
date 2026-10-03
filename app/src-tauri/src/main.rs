// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if molly_skills::is_cli_request() {
        if let Err(error)=mollycloud_lib::require_skills_cli() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        molly_skills::cli::main();
        return;
    }
    mollycloud_lib::run()
}
