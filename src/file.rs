use serde::Serialize;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path;

#[derive(Debug, Default, Clone, Serialize)]
pub struct FileInfo {
    pub inode: u64,
    pub size: u64,
    pub path: path::PathBuf,
}

pub(crate) fn is_path_valid<P: AsRef<path::Path>>(file: P) -> io::Result<bool> {
    let meta = fs::metadata(file)?;
    let tipo = meta.file_type();
    if tipo.is_block_device() | tipo.is_fifo() | tipo.is_char_device() {
        return Ok(false);
    }

    Ok(tipo.is_dir() | tipo.is_file())
}

pub fn walk_dir<P: AsRef<path::Path>>(dir: &P) -> PathIter {
    PathIter::new(dir)
}

pub struct PathIter {
    stack: Vec<PathSelection>,
    current: PathSelection,
}

impl PathIter {
    fn new<P>(path: &P) -> Self
    where
        P: AsRef<path::Path>,
    {
        let valid = check_if_file_is_valid(path);
        if !valid {
            return Self { stack: vec![], current: PathSelection::EMPTY };
        }

        if path.as_ref().is_file() {
            return Self {
                stack: vec![],
                current: PathSelection::FILE(Some(path.as_ref().to_owned())),
            };
        }
        let entry = path.as_ref().read_dir();
        if entry.is_err() {
            eprintln!(
                "There was an error when reading {}, skipping it",
                &path.as_ref().display()
            );
            return Self { stack: vec![], current: PathSelection::EMPTY };
        }

        Self {
            stack: vec![],
            current: PathSelection::FOLDER(entry.unwrap(), path.as_ref().to_owned()),
        }
    }
}

impl Iterator for PathIter {
    type Item = FileInfo;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.current.next() {
            let valid = check_if_file_is_valid(&path);
            if !valid {
                continue;
            }

            if path.is_file() {
                let metadata = path.metadata();
                if metadata.is_err() {
                    let file_ = &path.as_path().display();
                    eprintln!("Could not access metadata for file {}", &file_);
                    eprintln!("Skipping file {}", &file_);
                    continue;
                }

                let metadata = metadata.unwrap();
                let info = FileInfo { path, inode: metadata.ino(), size: metadata.size() };
                return Some(info);
            }

            let dir = path.read_dir();
            if dir.is_err() {
                eprintln!("There was an error when reading {}, skipping it", &path.display());
                continue;
            }
            self.stack.push(PathSelection::FOLDER(dir.unwrap(), path));
        }
        let new = self.stack.pop()?;
        self.current = new;
        self.next()
    }
}

// ----------
//  Internal
// ----------

#[derive(Debug)]
enum PathSelection {
    FILE(Option<path::PathBuf>),
    FOLDER(std::fs::ReadDir, path::PathBuf),
    EMPTY,
}

impl Iterator for PathSelection {
    type Item = path::PathBuf;
    fn next(&mut self) -> Option<Self::Item> {
        if let Self::EMPTY = self {
            return None;
        }

        if let Self::FILE(f) = self {
            return f.take();
        }

        if let Self::FOLDER(f, path) = self {
            let entry = f.next()?;
            if entry.is_err() {
                eprintln!("There was an error when reading the folder {}", &path.display());
                return None;
            }

            return Some(entry.unwrap().path());
        }
        None
    }
}

fn check_if_file_is_valid<P: AsRef<path::Path>>(dir: &P) -> bool {
    let valid = is_path_valid(dir);
    if valid.is_err() {
        eprintln!(
            "There was an error when checking whether the file {:?} is valid, skipping it",
            &dir.as_ref().display()
        );
        return false;
    }

    let result = valid.unwrap();
    if !result {
        eprintln!("File {:?} is not valid, skipping it", &dir.as_ref().display());
    }
    result
}
