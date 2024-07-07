pub mod cli;
pub mod cmp;
pub mod common;
pub mod config;
pub mod file;
use clap::Parser;
use cli::Cli;
use std::convert::TryInto;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let config: config::Config = cli.try_into()?;
    if config.verbose {
        println!("The config struct is: {:?}", &config);
    }

    if let Some(path) = &config.output {
        _ = std::fs::File::create(path)?;
    }

    let prep = common::preprocess(Some(&config.lhs), Some(&config.rhs))?;
    let mut cmp = cmp::Comparator::from_config(&config);
    let res = cmp.process_files(prep, config.chunks_only, config.verbose);
    let rpt = serde_json::to_string_pretty(&res).unwrap();

    if let Some(path) = &config.output {
        println!("Writing report to file '{}'", path.display());
        let file = std::fs::File::create(path)?;
        let mut file = std::io::BufWriter::new(file);
        file.write_all(rpt.as_bytes())?;
    } else {
        println!("{rpt}");
    }
    println!("rcompare complete!");
    Ok(())
}
