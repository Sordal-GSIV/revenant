use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "revenant", about = "Rust+Lua proxy for Simutronics games")]
pub struct Config {
    #[arg(long, default_value = "127.0.0.1:4900")]
    pub listen: String,
    #[arg(long)]
    pub account: String,
    #[arg(long)]
    pub password: String,
    #[arg(long, default_value = "GS3")]
    pub game: String,
    #[arg(long)]
    pub character: String,
    #[arg(long, default_value = "../scripts")]
    pub scripts_dir: String,
    #[arg(long, default_value = "revenant.db")]
    pub db_path: String,
}
