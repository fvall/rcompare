use crate::common::{FileSeparation, Preprocessed, Processed};
use crate::config::{Key, HASH_BUF_SIZE, READ_SIZE};
use crate::file::FileInfo;
use fasthash::MetroHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::prelude::*;
use std::path;

fn hash_file<P: AsRef<path::Path>>(path: &P, buf_size: Option<usize>) -> io::Result<Key> {
    let file = fs::File::open(&path)?;
    let mut reader = std::io::BufReader::with_capacity(HASH_BUF_SIZE, file);

    let mut buf = [0; 1024];
    let size = buf_size.unwrap_or(HASH_BUF_SIZE);
    let mut count = 0;
    let mut hasher = MetroHasher::default();
    while count < size {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        buf[..n].hash(&mut hasher);
        count += n;
    }

    let hash = hasher.finish();
    Ok(hash)
}

fn get_readers<P: AsRef<path::Path>, Q: AsRef<path::Path>>(
    a: &P,
    b: &Q,
) -> io::Result<(std::io::BufReader<fs::File>, std::io::BufReader<fs::File>)> {
    let file_a = fs::File::open(a);
    let file_b = fs::File::open(b);

    if let Err(err) = file_a {
        eprintln!("File {} raised an error", a.as_ref().to_str().unwrap());
        eprintln!("Error: {:?}", &err);
        return Err(err);
    }

    if let Err(err) = file_b {
        eprintln!("File {} raised an error\n", b.as_ref().to_str().unwrap());
        eprintln!("Error: {:?}", &err);
        return Err(err);
    }

    let file_a = file_a.unwrap();
    let file_b = file_b.unwrap();
    let reader_a = std::io::BufReader::with_capacity(READ_SIZE, file_a);
    let reader_b = std::io::BufReader::with_capacity(READ_SIZE, file_b);

    Ok((reader_a, reader_b))
}

pub fn compare_file_seq<P, Q>(lhs: &P, rhs: &Q) -> io::Result<bool>
where
    P: AsRef<path::Path>,
    Q: AsRef<path::Path>,
{
    let mut buf_lhs: [u8; READ_SIZE] = [0; READ_SIZE];
    let mut buf_rhs: [u8; READ_SIZE] = [0; READ_SIZE];

    let (mut reader_lhs, mut reader_rhs) = get_readers(lhs, rhs)?;
    let mut bts_lhs: usize;
    let mut bts_rhs: usize;

    loop {
        bts_lhs = reader_lhs.read(&mut buf_lhs)?;
        bts_rhs = reader_rhs.read(&mut buf_rhs)?;

        if (bts_lhs != bts_rhs) || (buf_lhs[..bts_lhs] != buf_rhs[..bts_rhs]) {
            return Ok(false);
        }

        if (bts_rhs == 0) | (bts_lhs == 0) {
            break;
        }
    }

    Ok((bts_rhs == 0) & (bts_lhs == 0))
}

pub fn compare_file_full<P, Q>(lhs: &P, rhs: &Q, buf_lhs: &mut Vec<u8>, buf_rhs: &mut Vec<u8>) -> io::Result<bool>
where
    P: AsRef<path::Path>,
    Q: AsRef<path::Path>,
{
    buf_lhs.clear();
    buf_rhs.clear();

    let (mut reader_lhs, mut reader_rhs) = get_readers(lhs, rhs)?;
    let bts_lhs = reader_lhs.read_to_end(buf_lhs)?;
    let bts_rhs = reader_rhs.read_to_end(buf_rhs)?;

    if (bts_lhs != bts_rhs) || (buf_lhs[..bts_lhs] != buf_rhs[..bts_rhs]) {
        return Ok(false);
    }

    Ok(true)
}

pub fn process_files(mut prep: Preprocessed) -> Processed {
    let mut bufa = Vec::with_capacity(READ_SIZE);
    let mut bufb = Vec::with_capacity(READ_SIZE);

    for dupes in prep.to_process.drain(..) {
        let mut sep = separate_files(&dupes, &prep.info, &mut bufa, &mut bufb);
        prep.same.append(&mut sep.same);
        prep.unique.append(&mut sep.unique);
        if !sep.errors.is_empty() {
            for idx in sep.errors.drain(..) {
                let fl = prep.info.get(idx);
                if fl.is_none() {
                    eprintln!("Unable to get information for index {}", idx);
                    continue;
                }

                let fl = fl.unwrap();
                eprintln!("There was an error when processing file {}", &fl.path.display());
            }
        }
    }

    // just for convenience
    prep.same.sort();
    prep.unique.sort();
    prep.zero.sort();

    Processed { info: prep.info, same: prep.same, zero: prep.zero, unique: prep.unique }
}

pub fn separate_files(dupes: &[usize], list: &[FileInfo], bufa: &mut Vec<u8>, bufb: &mut Vec<u8>) -> FileSeparation {
    let mut map: Vec<(Key, Vec<Vec<usize>>)> = Vec::with_capacity(dupes.len() / 2 + 1);
    let mut errors: Vec<usize> = vec![];

    let size: u64;
    if dupes.is_empty() {
        size = 0;
    } else {
        size = list[dupes[0]].size;
    }

    let full: bool;
    const MAX_FILE_SIZE: u64 = 1024u64.pow(3);
    if size < 2 * READ_SIZE as u64 {
        full = false;
    } else if size < MAX_FILE_SIZE {
        full = true;
    } else {
        full = false;
    }

    for idx in dupes.iter() {
        let fl = list.get(*idx);
        if fl.is_none() {
            eprintln!("Could not find file at position {}", &idx);
            errors.push(*idx);
        }

        let fl = fl.unwrap();
        let hash = hash_file(&fl.path, None);
        if let Err(err) = hash {
            eprintln!("Unable to hash file {}", &fl.path.display());
            eprintln!("Error: {:?}", err);
            errors.push(*idx);
            continue;
        }

        let key = hash.unwrap();
        let pos = map.iter().position(|(k, _)| k == &key);

        // if there are no groups, we just insert one
        if pos.is_none() {
            map.push((key, vec![vec![*idx]]));
            continue;
        }

        // if a group exists we check if the file actually belongs to any of them
        let (_, groups) = map.get_mut(pos.unwrap()).unwrap();
        let mut matched: bool = false;
        for group in groups.iter_mut() {
            // equality is transitive so just needs to check the first entry of the group
            let found = &list[group[0]];

            // if the inode is the same, the files must be equal
            if found.inode == fl.inode {
                group.push(*idx);
                matched = true;
                break;
            }

            // if the inode is not the same we compare the whole file
            println!("Comparing {} vs {}", &fl.path.display(), &found.path.display());
            let check = match full {
                true => compare_file_full(&fl.path, &found.path, bufa, bufb),
                false => compare_file_seq(&fl.path, &found.path),
            };
            if let Err(err) = check {
                eprintln!(
                    "There was an error when checking file {} vs {}",
                    &fl.path.display(),
                    found.path.display()
                );
                eprintln!("Error: {}", err);
                eprintln!("Skipping file {}", &fl.path.display());
                errors.push(*idx);
                continue;
            }

            if let Ok(ck) = check {
                if ck {
                    group.push(*idx);
                    matched = true;
                    break;
                }
            }
        }

        // at this stage there was a hash collision but it did not match any of the groups
        // we then create a new group under the same hash

        if !matched {
            groups.push(vec![*idx]);
        }
    }

    let mut same: Vec<Vec<usize>> = vec![];
    let mut unique: Vec<usize> = vec![];
    for (_, mut value) in map.drain(..) {
        for group in value.drain(..) {
            match group.len() {
                0 => panic!("Vector cannot be empty here"),
                1 => unique.push(group[0]),
                _ => same.push(group),
            }
        }
    }
    FileSeparation { same, unique, errors }
}
