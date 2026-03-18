use anyhow::Result;
use clap::Parser;
use codexchat_cli::{Cli, run};

#[tokio::main]
async fn main() -> Result<()> {
    run(Cli::parse()).await
}
