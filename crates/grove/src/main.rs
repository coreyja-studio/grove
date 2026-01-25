use clap::Parser;

#[derive(Parser)]
#[command(name = "grove", version, about = "Manage a grove of git repositories")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    // Placeholder for future commands
}

fn main() {
    let _cli = Cli::parse();
    println!("grove - coming soon");
}
