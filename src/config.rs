use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "revenant", about = "Rust+Lua proxy for Simutronics games")]
pub struct Config {
    #[arg(long, default_value = "127.0.0.1:4900")]
    pub listen: String,
    #[arg(long)]
    pub account: Option<String>,
    #[arg(long)]
    pub password: Option<String>,
    #[arg(long, default_value = "GS3")]
    pub game: String,
    #[arg(long)]
    pub character: Option<String>,
    #[arg(long, default_value = "./scripts")]
    pub scripts_dir: String,
    #[arg(long, default_value = "revenant.db")]
    pub db_path: String,
    #[arg(long)]
    pub map_path: Option<String>,
    #[arg(long, default_value_t = false)]
    pub monitor: bool,
    #[arg(long, default_value_t = false)]
    pub without_frontend: bool,
    #[arg(long)]
    pub detachable_client_port: Option<u16>,
    #[arg(long, default_value = "127.0.0.1")]
    pub detachable_client_host: String,
    #[arg(long, default_value_t = false)]
    pub reconnect: bool,
    #[arg(long, default_value_t = 30)]
    pub reconnect_delay: u64,
    #[arg(skip)]
    pub session: Option<crate::eaccess::Session>,
    #[arg(skip)]
    pub frontend: String,
    #[arg(skip)]
    pub custom_launch: Option<String>,
    #[arg(skip)]
    pub custom_launch_dir: Option<String>,
}
