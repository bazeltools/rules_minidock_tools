use clap::Parser;
use rules_minidock_tools::{merge_main, Opt};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();
    merge_main(opt).await
}
