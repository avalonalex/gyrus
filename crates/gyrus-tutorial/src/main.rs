//! `gyrus-tutorial` — a course in BrainFuck, taught by running it.
//!
//! Each lesson explains one idea, hands the learner a program that demonstrates
//! it, and asks for a variation. The program runs on the same interpreter the
//! rest of gyrus uses; the tutorial's only addition is a hook that records
//! every step, so the learner can walk backwards through a loop as easily as
//! forwards. That is the thing that makes `[->+<]` legible, and it is only
//! affordable because a lesson's tape is sixteen cells rather than thirty
//! thousand.

mod app;
mod editor;
mod lesson;
mod trace;
mod ui;

use clap::Parser;
use gyrus_tui::TerminalGuard;

use app::App;
use lesson::LESSONS;

#[derive(Parser)]
#[command(name = "gyrus-tutorial")]
#[command(about = "Learn BrainFuck by running it", long_about = None)]
#[command(after_help = "Press F1 inside the tutorial for the full key list.")]
struct Cli {
    /// Start at this lesson number (0 is the first)
    #[arg(long, value_name = "N", default_value = "0")]
    lesson: usize,

    /// List the lessons and exit
    #[arg(long)]
    list: bool,
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    if cli.list {
        for (index, lesson) in LESSONS.iter().enumerate() {
            println!("{index:>2}  {}", lesson.title);
        }
        return Ok(());
    }

    if cli.lesson >= LESSONS.len() {
        return Err(format!(
            "Error: there are {} lessons, numbered 0 to {}",
            LESSONS.len(),
            LESSONS.len() - 1
        ));
    }

    let (_guard, mut terminal) = TerminalGuard::enter()
        .map_err(|error| format!("Error: could not set up the terminal: {error}"))?;

    let mut app = App::new(cli.lesson);
    ui::run(&mut terminal, &mut app).map_err(|error| format!("Error: terminal failure: {error}"))
}
