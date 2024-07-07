use crate::common::{stringify_bytes, FileSeparation, Preprocessed, Processed};
use crate::config::{Config, Key, HASH_BUF_SIZE};
use crate::file::FileInfo;
use fasthash::MetroHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

fn hash_file<P: AsRef<Path>>(path: &P, buf_size: Option<usize>) -> io::Result<Key> {
    let file = File::open(path)?;
    let size = buf_size.unwrap_or(HASH_BUF_SIZE);
    let mut reader = std::io::BufReader::with_capacity(size, file);
    let mut hasher = MetroHasher::default();
    let mut buf = [0; 1024];
    let mut count = 0;
    while count < size {
        let mut n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        n = n.min(size - count);
        buf[..n].hash(&mut hasher);
        count += n;
    }

    let hash = hasher.finish();
    Ok(hash)
}

fn get_readers<P: AsRef<Path>, Q: AsRef<Path>>(
    a: &P,
    b: &Q,
    read_size: usize,
) -> io::Result<(BufReader<File>, BufReader<File>)> {
    let file_a = File::open(a);
    let file_b = File::open(b);

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
    let reader_a = BufReader::with_capacity(read_size, file_a);
    let reader_b = BufReader::with_capacity(read_size, file_b);

    Ok((reader_a, reader_b))
}
pub struct Comparator {
    read_size: usize,
    hash_size: usize,
    max_file_size: u64,
    bufa: Vec<u8>,
    bufb: Vec<u8>,
}

impl Comparator {
    pub fn new(read_size: usize, hash_size: usize, max_file_size: u64) -> Self {
        let bufa = Vec::with_capacity(read_size);
        let bufb = Vec::with_capacity(read_size);
        Self { read_size, hash_size, bufa, bufb, max_file_size }
    }

    pub fn from_config(config: &Config) -> Self {
        Comparator::new(config.read_size, config.hash_size, config.max_file_size)
    }

    fn compare_file_seq<P, Q>(&mut self, lhs: &P, rhs: &Q) -> io::Result<bool>
    where
        P: AsRef<Path> + ?Sized,
        Q: AsRef<Path> + ?Sized,
    {
        let (mut reader_lhs, mut reader_rhs) = get_readers(&lhs, &rhs, self.read_size)?;
        let mut bts_lhs: usize;
        let mut bts_rhs: usize;

        loop {
            bts_lhs = reader_lhs.read(self.bufa.as_mut_slice())?;
            bts_rhs = reader_rhs.read(self.bufb.as_mut_slice())?;

            if (bts_lhs != bts_rhs) || (self.bufa[..bts_lhs] != self.bufb[..bts_rhs]) {
                return Ok(false);
            }

            if (bts_rhs == 0) | (bts_lhs == 0) {
                break;
            }
        }

        Ok((bts_rhs == 0) & (bts_lhs == 0))
    }

    fn compare_file_full<P, Q>(&mut self, lhs: &P, rhs: &Q) -> io::Result<bool>
    where
        P: AsRef<Path> + ?Sized,
        Q: AsRef<Path> + ?Sized,
    {
        self.bufa.clear();
        self.bufb.clear();

        let (mut reader_lhs, mut reader_rhs) = get_readers(&lhs, &rhs, self.read_size)?;
        let bts_lhs = reader_lhs.read_to_end(&mut self.bufa)?;
        let bts_rhs = reader_rhs.read_to_end(&mut self.bufb)?;

        if (bts_lhs != bts_rhs) || (self.bufa[..bts_lhs] != self.bufb[..bts_rhs]) {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn hash_file<P: AsRef<Path>>(&self, path: &P) -> io::Result<Key> {
        hash_file(&path, Some(self.hash_size))
    }

    fn separate_files(
        &mut self,
        dupes: &[usize],
        list: &[FileInfo],
        compare: fn(&mut Self, &Path, &Path) -> io::Result<bool>,
        _verbose: bool,
        total: usize,
        progress: &mut usize,
    ) -> FileSeparation {
        let mut map: Vec<(Key, Vec<Vec<usize>>)> = Vec::with_capacity(dupes.len() / 2 + 1);
        let mut errors: Vec<usize> = vec![];

        for idx in dupes.iter() {
            *progress += 1;
            let fl = list.get(*idx);
            if fl.is_none() {
                eprintln!("Could not find file at position {}", &idx);
                errors.push(*idx);
            }

            let fl = fl.unwrap();
            let hash = self.hash_file(&fl.path);
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
                // just needs to check the first entry of the group
                let found = list.get(group[0]);
                if found.is_none() {
                    eprintln!(
                        "There was an error when getting FileInfo: index {} was used but it is not in the vector",
                        group[0]
                    );
                    continue;
                }

                let found = found.unwrap();
                if cfg!(target_family = "unix") {
                    // if the inode is the same, the files must be equal
                    if found.inode == fl.inode {
                        group.push(*idx);
                        matched = true;
                        break;
                    }
                }

                // if the inode is not the same we compare the whole file
                let pct = (*progress * 100) / total;
                let msg = format!(
                    "Progress: {}% --- Comparing {} vs {}",
                    pct,
                    &fl.path.display(),
                    &found.path.display()
                );
                print_same_line(&msg, pct < 100);
                let check = compare(self, &fl.path, &found.path);
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

    pub fn process_files(&mut self, mut prep: Preprocessed, chunks_only: bool, verbose: bool) -> Processed {
        let mut capa = self.bufa.capacity();
        let mut capb = self.bufb.capacity();
        let mut cmp: fn(&mut Self, &Path, &Path) -> io::Result<bool>;
        let info = prep.info;

        let mut progress = 0;
        let total = prep.to_process.iter().map(|v| v.len()).sum::<usize>();
        for dupes in prep.to_process.iter() {
            let size = dupes
                .first()
                .map(|&idx| info.get(idx).map(|i| i.size).unwrap_or(0))
                .unwrap_or(0);

            let full = (!chunks_only) && (size > 2 * self.read_size as u64) && (size < self.max_file_size);
            cmp = if full {
                Self::compare_file_full
            } else {
                // - We need to check if buffers have enough size to read sequentially, since
                // - we clear the vector when we run the full comparison
                while self.bufa.len() < self.read_size {
                    self.bufa.push(0);
                }

                while self.bufb.len() < self.read_size {
                    self.bufb.push(0);
                }
                Self::compare_file_seq
            };

            let mut sep = self.separate_files(dupes, &info, cmp, verbose, total, &mut progress);
            if verbose {
                if capa < self.bufa.capacity() {
                    println!(
                        "We needed to grow buffer A, additional {}",
                        stringify_bytes(self.bufa.capacity() - capa)
                    );
                    capa = self.bufa.capacity();
                    println!("Buffer A size is: {}", stringify_bytes(self.bufa.len()));
                }

                if capb < self.bufb.capacity() {
                    println!(
                        "We needed to grow buffer B, additional {}",
                        stringify_bytes(self.bufb.capacity() - capb)
                    );
                    capb = self.bufb.capacity();
                    println!("Buffer B size is: {}", stringify_bytes(self.bufb.len()));
                }
            }

            prep.same.append(&mut sep.same);
            prep.unique.append(&mut sep.unique);
            for idx in sep.errors.drain(..) {
                let fl = info.get(idx);
                if fl.is_none() {
                    eprintln!("Unable to get information for index {}", idx);
                    continue;
                }

                let fl = fl.unwrap();
                eprintln!("There was an error when processing file {}", &fl.path.display());
            }
        }

        Processed { info, same: prep.same, zero: prep.zero, unique: prep.unique }
    }
}

fn print_same_line(s: &str, clear_line: bool) {
    print!("{}", s);
    let res = std::io::stdout().flush();
    if res.is_err() {
        panic!("Unable to print to stdout");
    }
    if clear_line {
        print!("\x1B[2K");
        print!("\r");
    } else {
        println!();
    }
}
