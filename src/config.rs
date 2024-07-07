pub(crate) type Key = u64;
pub const READ_SIZE: usize = 64 * 1024;
pub const HASH_BUF_SIZE: usize = 4 * 1024;
pub const MAX_FILE_SIZE: u64 = 1024u64.pow(3);

#[derive(Debug)]
pub struct Config {
    pub lhs: std::path::PathBuf,
    pub rhs: std::path::PathBuf,
    pub output: Option<std::path::PathBuf>,
    pub verbose: bool,
    pub read_size: usize,
    pub hash_size: usize,
    pub max_file_size: u64,
    pub chunks_only: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lhs: std::env::current_dir().unwrap(),
            rhs: std::env::current_dir().unwrap(),
            output: None,
            verbose: false,
            read_size: READ_SIZE,
            hash_size: HASH_BUF_SIZE,
            max_file_size: MAX_FILE_SIZE,
            chunks_only: false,
        }
    }
}
