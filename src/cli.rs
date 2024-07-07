use crate::common::stringify_bytes;
use crate::config::{Config, HASH_BUF_SIZE, MAX_FILE_SIZE, READ_SIZE};
use clap::Parser;
use std::convert::TryFrom;

#[derive(Debug, Parser)]
#[command(name = "rcompare")]
pub(crate) struct Cli {
    #[arg(help = "the first path - default is current directory")]
    pub lhs: Option<String>,
    #[arg(help = "the second path - default is the first path")]
    pub rhs: Option<String>,
    #[arg(short, help = "output path")]
    pub output: Option<String>,
    #[arg(short, long, help = "print information as the program runs")]
    pub verbose: bool,

    #[arg(long, value_name = "max_file_size", help = &format!("maximum file size allowed to read it entirely in memory - default: {}", stringify_bytes(MAX_FILE_SIZE as usize)))]
    pub max_file_size: Option<u64>,

    #[arg(long, value_name = "read_size", help = &format!("read block size - default: {}", stringify_bytes(READ_SIZE)))]
    pub read_size: Option<u64>,

    #[arg(long, value_name = "hash_size", help = &format!("how many bytes to read for hash calculation - default: {}", stringify_bytes(HASH_BUF_SIZE)))]
    pub hash_size: Option<u64>,

    #[arg(long, value_name = "chunks_only", help = "disable reading the entire file into memory")]
    pub chunks_only: bool,
}

impl TryFrom<Cli> for Config {
    type Error = std::io::Error;
    fn try_from(value: Cli) -> std::io::Result<Self> {
        let lhs = match value.lhs {
            Some(s) => std::path::Path::new(s.as_str()).to_path_buf(),
            None => std::env::current_dir().expect("Cannot get current directory"),
        };

        let verbose = value.verbose;
        let path = lhs.canonicalize();
        if let Err(e) = path {
            eprintln!("There was an error when standardizing the path '{}'. Error: {}", lhs.display(), &e);
            return Err(e);
        }

        let lhs = path.unwrap();
        if verbose {
            println!("The standardized lhs path is {}", lhs.display());
        }

        let rhs = match value.rhs {
            Some(s) => std::path::Path::new(s.as_str()).to_path_buf(),
            None => lhs.clone(),
        };

        let path = rhs.canonicalize();
        if let Err(e) = path {
            eprintln!("There was an error when standardizing the path '{}'. Error: {}", rhs.display(), &e);
            return Err(e);
        }

        let rhs = path.unwrap();
        if verbose {
            println!("The standardized rhs path is {}", rhs.display());
        }

        let output = value.output.map(|s| std::path::Path::new(s.as_str()).to_path_buf());
        let chunks_only = value.chunks_only;

        let read_size = value.read_size.map(|u| u as usize).unwrap_or(READ_SIZE);
        let hash_size = value.hash_size.map(|u| u as usize).unwrap_or(HASH_BUF_SIZE);
        let max_file_size = value.max_file_size.unwrap_or(MAX_FILE_SIZE);

        Ok(Config {
            lhs,
            rhs,
            verbose,
            read_size,
            hash_size,
            chunks_only,
            max_file_size,
            output,
        })
    }
}
