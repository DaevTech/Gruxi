
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct CommandLineArgs {
    #[arg(short, long, value_parser = ["DEV", "DEBUG", "PRODUCTION", "SPEEDTEST"])]
    pub opmode: Option<String>,
}
