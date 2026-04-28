mod app_state;
mod commands;
mod storage;
mod ticktick;
mod todo;
mod ui;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut state = storage::load();

    match commands::handle_args(&args, &mut state) {
        commands::CommandOutcome::OpenUi => {
            if let Err(e) = ui::run(&mut state) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        commands::CommandOutcome::Exit(code) => {
            if code != 0 {
                std::process::exit(code);
            }
        }
    }
}
