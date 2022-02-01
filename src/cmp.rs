use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix::fs::FileTypeExt;
use std::path;
use std::convert::TryInto;
use md5::{Md5, Digest};

const SIZE: usize = 16;
type Key = [u8; SIZE];


fn check_file<P: AsRef<path::Path>>(file: P) -> io::Result<()> {
    let meta = fs::metadata(file)?;
    let tipo = meta.file_type();
    if tipo.is_block_device() | tipo.is_fifo() | tipo.is_char_device() {
        return Err(io::Error::new(io::ErrorKind::Other, "incorrect type"));
    }

    Ok(())
}

pub fn compare_file<P: AsRef<path::Path>, Q: AsRef<path::Path>>(a: &P, b: &Q) -> io::Result<bool> {
    check_file(&a)?;
    check_file(&b)?;

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

    const SIZE: usize = 10 * 1024;
    let mut contents_a: [u8; SIZE] = [0; SIZE];
    let mut contents_b: [u8; SIZE] = [0; SIZE];

    let mut file_a = file_a.unwrap();
    let mut file_b = file_b.unwrap();
    let mut bta: usize;
    let mut btb: usize;

    loop {
        bta = file_a.read(&mut contents_a)?;
        btb = file_b.read(&mut contents_b)?;

        if contents_a != contents_b {
            return Ok(false);
        }
        if (btb == 0) | (bta == 0) {
            break;
        }
    }

    Ok((btb == 0) & (bta == 0))
}

#[derive(Copy, Clone, Debug)]
pub enum Side {
    LEFT,
    RIGHT
}

#[derive(Debug)]
pub struct FileC {
    pub name: String,
    pub side: Side
}

fn hash_file<P : AsRef<path::Path>>(path: &P) -> io::Result<Key> {

    let mut file = fs::File::open(&path)?;
    let mut buf = [0; 10 * 1024];
    file.read(&mut buf)?;


    let mut hasher = Md5::new();
    hasher.update(&buf);

    let hash = hasher.finalize();
    let res: Key = hash.as_slice().try_into().expect("Wrong length");
    Ok(res)
}

#[derive(Debug)]
pub struct FolderC {
    pub map: HashMap<Key, Vec<Vec<FileC>>>
}

impl Default for FolderC {
    fn default() -> Self {
        Self::new()
    }
}

impl FolderC {

    pub fn new() -> Self {
        FolderC{map: HashMap::new()}
    }

    fn add_to_map<P>(&mut self, hash: Key, path: &P, side: Side) where P: AsRef<path::Path> {

        let name = path.as_ref().to_string_lossy().into_owned();
        let new = FileC{name: name.clone(), side};
        if self.map.contains_key(&hash) {

            // ------------------------------------------------------------
            // if the hash is in the map, we need to manage the collisions
            // ------------------------------------------------------------

            for choices in self.map.get_mut(&hash).unwrap() {
                let check = compare_file(&choices[0].name, path);
                if check.is_err() {
                    eprintln!("Cannot compare files {} and {}", &choices[0].name, &name);
                    eprintln!("Skipping {}", name);
                    return
                }

                let check = check.unwrap();
                if check {
                    choices.push(new);
                    return
                }
            }

            // ------------------------------------------------------------------------
            // in this case it means there is a collision but the files are not equal
            // ------------------------------------------------------------------------

            self.map.get_mut(&hash).unwrap().push(vec![new]);
            return;
        }

        // ------------------------------------------------------------
        // if the hash is not in the map, we need to create a new entry
        // ------------------------------------------------------------
        
        self.map.insert(hash, vec![vec![new]]);

    }

    pub fn add_file<P>(&mut self, file: &P, side: Side) where P: AsRef<path::Path> {
        if file.as_ref().is_dir() {
            for d in fs::read_dir(file).unwrap() {
                let entry = d.unwrap();
                let path = entry.path();
                self.add_file(&path, side);
            }
            return;
        }

        let hash = hash_file(file);
        if let Ok(h) = hash {
            // let name = file.as_ref().to_string_lossy().into_owned();
            // println!("Processing {}...", &name);
            self.add_to_map(h, file, side);
            return;
        }

        let path = file.as_ref().to_string_lossy().into_owned();
        eprintln!("Cannot hash file {}, skipping...", path);

    }

}

fn print_files(header: &str, files: &[String]) -> String {
        
    let mut msg = String::new();
    msg.push_str(header);
    msg.push('\n');

    if files.is_empty() {
        msg.push_str("[]");
        msg.push('\n');
    } else {
        msg.push('[');
        msg.push('\n');
        for s in files {
            msg.push_str("  ");
            msg.push_str(s);
            msg.push(';');
            msg.push('\n');
        }
        msg.push(']');
        msg.push('\n');
    }
    
    msg
}

pub fn print_report<K> (mut map: HashMap<K, Vec<Vec<FileC>>>, left: &str, right: &str) {

    let mut same: Vec<String> = Vec::new();
    let mut missing_left: Vec<String> = Vec::new();
    let mut missing_right: Vec<String> = Vec::new();

    for (_, mut val) in map.drain() {
        for mut vec in val.drain(..) {
            if vec.len() == 1 {
                let file = vec.pop().unwrap();
                match file.side {
                    Side::LEFT => {missing_left.push(file.name)},
                    Side::RIGHT => {missing_right.push(file.name)},
                }
                continue;
            }

            let mut set = vec.drain(..).map(|f| f.name).collect::<HashSet<String>>().drain().collect::<Vec<String>>();
            set.sort();
            same.push(set.join(", "));
        }
    }

    same.sort();
    missing_left.sort();
    missing_right.sort();

    let mut msg = "Comparison report\n".to_string();
    for _ in 0..msg.len() {
        msg.push('-');
    }

    msg.push('\n');
    msg.push_str(&print_files("Same contents:", &same));
    
    
    msg.push('\n');
    msg.push_str(&print_files(
        &format!("Missing from {}:", left),
        &missing_right
    ));
    
    msg.push('\n');
    msg.push_str(&print_files(
        &format!("Missing from {}:", right),
        &missing_left
    ));

    println!("{}", msg);
}