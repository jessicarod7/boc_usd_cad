use boc_usd_cad::{Cli, retrieve_rates};
use clap::Parser;

fn main() {
    let args = Cli::parse();
    for obs in retrieve_rates(&args).unwrap() {
        println!("{}: {}", obs.d, obs.fx.v);
    }
}
