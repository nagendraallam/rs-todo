mod storage;
mod todo;
mod ui;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut list = storage::load();

    if args.is_empty() {
        // No args → open the interactive TUI viewer
        if let Err(e) = ui::run(&mut list) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    } else {
        // Any text → create a new task
        let title = args.join(" ");
        list.add(title.clone());
        match storage::save(&list) {
            Ok(_)  => println!("✓  Added: {title}"),
            Err(e) => {
                eprintln!("Failed to save: {e}");
                std::process::exit(1);
            }
        }
    }
}
