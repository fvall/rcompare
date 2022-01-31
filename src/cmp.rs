use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix::fs::FileTypeExt;
use std::path;
use std::rc::Rc;

const SIZE: usize = 10 * 1024;
type Key = [u8; SIZE];
type CompareOutput = io::Result<HashMap<String, Rc<RefCell<HashSet<String>>>>>;

#[derive(Debug)]
pub struct FileComparison {
    pub name_left: String,
    pub name_right: String,
    pub left: HashMap<Rc<String>, Option<usize>>,
    pub right: HashMap<Rc<String>, Option<usize>>,
    pub files: HashSet<Rc<String>>,
    pub links: Vec<HashSet<Rc<String>>>,
}

#[derive(Debug)]
pub struct FileLink {
    pub files: HashSet<Rc<String>>,
    pub links: Vec<(Rc<String>, Rc<String>, bool)>,
}

impl FileLink {
    pub fn new() -> Self {
        FileLink{files: HashSet::new(), links: Vec::new()}
    }

    fn add(&mut self, file: &str) -> Rc<String> {
        let file = String::from(file);
        if self.files.contains(&file) {
            return self.files.get(&file).unwrap().clone();
        }

        let rc = Rc::new(file);
        self.files.insert(rc.clone());
        return rc;
    }

    pub fn add_link(&mut self, left: &str, right: &str, same: bool) {
        let left = self.add(left);
        let right = self.add(right);
        self.links.push((left, right, same))
    }
}

impl FileComparison {
    pub fn new(name_left: &str, name_right: &str) -> Self {
        FileComparison {
            name_left: name_left.to_owned(),
            name_right: name_right.to_owned(),
            left: HashMap::new(),
            right: HashMap::new(),
            files: HashSet::new(),
            links: Vec::new(),
        }
    }

    fn add_to_map(files: &mut HashSet<Rc<String>>, map: &mut HashMap<Rc<String>, Option<usize>>, file: &str) {
 
        if file == "" {
            return;
        }

        let file = String::from(file);
        if !map.contains_key(&file) {
            let rc = Rc::new(file);
            files.insert(rc.clone());
            map.insert(rc.clone(), None);
        }
    }

    pub fn add_left(&mut self, file: &str) {
        FileComparison::add_to_map(&mut self.files, &mut self.left, file)
    }

    pub fn add_right(&mut self, file: &str) {
        FileComparison::add_to_map(&mut self.files, &mut self.right, file)
    }

    pub fn link_files(&mut self, left: &str, right: &str) {
        self.add_left(left);
        self.add_right(right);

        let left = String::from(left);
        let right = String::from(right);

        let rc_left = self.files.get(&left).unwrap();
        let rc_right = self.files.get(&right).unwrap();

        let pos_left = self.left.get_mut(&left).unwrap();
        let pos_right = self.right.get_mut(&right).unwrap();

        let mut pos: usize = 0;
        let mut update_indices = false;
        if pos_left.is_some() & pos_right.is_some() {
            if pos_left.unwrap() != pos_right.unwrap() {
                // Move values to left index

                let mut v: Vec<_> = self.links[pos_right.unwrap()].drain().collect();
                for file in v.drain(..) {
                    self.links[pos_left.unwrap()].insert(file.clone());
                }

                update_indices = true;
            }

            pos = pos_left.unwrap();
        }

        if pos_left.is_none() & pos_right.is_none() {
            let set: HashSet<Rc<String>> = HashSet::new();
            self.links.push(set);
            pos = self.links.len() - 1;
            pos_left.replace(pos);
            pos_right.replace(pos);
        }

        if pos_left.is_none() & pos_right.is_some() {
            pos = pos_right.unwrap();
            pos_left.replace(pos);
        }

        if pos_left.is_some() & pos_right.is_none() {
            pos = pos_left.unwrap();
            pos_right.replace(pos);
        }
        if self.links.len() == 0 {
            panic!("Vector of links should not be empty at this stage");
        }

        self.links[pos].insert(rc_left.clone());
        self.links[pos].insert(rc_right.clone());

        if update_indices {
            let v: Vec<_> = self.links[pos].iter().collect();
            for val in v {
                if self.left.contains_key(val) {
                    self.left.insert(val.clone(), Some(pos));
                }
                if self.right.contains_key(val) {
                    self.right.insert(val.clone(), Some(pos));
                }
            }
        }
    }

    pub fn merge_link(&mut self, link: &mut FileLink) {

        for (left, right, same) in link.links.drain(..) {
            if same {
                self.link_files(&left, &right);
                continue
            } else {
                self.add_left(&left);
                self.add_right(&right);
            }
        }

    }

    fn split_files(
        &self,
        map: &HashMap<Rc<String>, Option<usize>>,
        same: &mut Vec<String>,
        seen: &mut HashSet<usize>,
    ) -> Vec<String> {

        let mut missing: Vec<String> = Vec::new();

        for (key, val) in map.iter() {
            if val.is_none() {
                missing.push((**key).clone());
                continue;
            }

            let pos = val.unwrap();
            if seen.contains(&pos) {
                continue;
            }

            let set = &self.links[pos];
            if set.len() > 0 {
                let mut link = set.iter().map(|x| &***x).collect::<Vec<&str>>();
                link.sort();
                same.push(link.join(", "));
                seen.insert(pos);
            }
        }

        return missing;
    }

    fn print_files(header: &str, files: &Vec<String>) -> String {
        
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
                msg.push_str(&s);
                msg.push(';');
                msg.push('\n');
            }
            msg.push(']');
            msg.push('\n');
        }
        
        msg
    }

    pub fn print(&self) {

        let mut same: Vec<String> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        
        let mut missing_left = self.split_files(&self.left, &mut same, &mut seen);
        let mut missing_right = self.split_files(&self.right, &mut same, &mut seen);

        missing_left.sort();
        missing_right.sort();
        same.sort();

        let mut msg = format!("Comparison report\n");
        for _ in 0..msg.len() {
            msg.push('-');
        }
    
        msg.push('\n');
        msg.push_str(&FileComparison::print_files("Same contents:", &same));
        
        
        msg.push('\n');
        msg.push_str(&FileComparison::print_files(
            &format!("Missing from {}:", self.name_left),
            &missing_right
        ));
        
        msg.push('\n');
        msg.push_str(&FileComparison::print_files(
            &format!("Missing from {}:", self.name_right),
            &missing_left
        ));

        println!("{}", msg);

    }
}

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

    if file_a.is_err() {
        let err = file_a.unwrap_err();
        eprintln!("File {} raised an error", a.as_ref().to_str().unwrap());
        eprintln!("Error: {:?}", &err);
        return Err(err);
    }

    if file_b.is_err() {
        let err = file_b.unwrap_err();
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

    let file = fs::File::open(&path);
    if file.is_err() {
        // eprintln!("Cannot open file {}", &path);
        return Err(file.unwrap_err());
    }

    let mut file = file.unwrap();
    let mut buf: Key = [0; SIZE];
    file.read(&mut buf)?;
    Ok(buf)
}

#[derive(Debug)]
pub struct FolderC {
    pub map: HashMap<Key, Vec<Vec<FileC>>>
}

impl FolderC {

    pub fn new() -> Self {
        return FolderC{map: HashMap::new()};
    }

    fn add_to_map<P>(&mut self, hash: Key, path: &P, side: Side) where P: AsRef<path::Path> {

        let name = path.as_ref().to_string_lossy().into_owned();
        let new = FileC{name: name.clone(), side: side};
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

fn print_files(header: &str, files: &Vec<String>) -> String {
        
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
            msg.push_str(&s);
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

    let mut msg = format!("Comparison report\n");
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

pub fn compare_folder_2<P: AsRef<path::Path>, Q: AsRef<path::Path>>(a: &P, b: &Q) -> io::Result<FileLink> {

    let mut res = FileLink::new();

    if a.as_ref().is_dir() {
        for d in fs::read_dir(a).unwrap() {
            let entry = d.unwrap();
            let path = entry.path();
            
            let mut other = compare_folder_2(&path, b)?;
            for (left, right, same) in other.links.drain(..) {
                res.add_link(&left, &right, same);
            }
        }

        return Ok(res);
    }

    if b.as_ref().is_dir() {
        for d in fs::read_dir(b).unwrap() {
            let entry = d.unwrap();
            let path = entry.path();
            
            let mut other = compare_folder_2(a, &path)?;
            for (left, right, same) in other.links.drain(..) {
                res.add_link(&left, &right, same);
            }
        }

        return Ok(res);
    }

    let result = compare_file(a, b);
    if result.is_err() {
        eprintln!(
            "Cannot compare files {} and {}",
            a.as_ref().to_str().unwrap(),
            b.as_ref().to_str().unwrap()
        );
        eprintln!("Error msg: {:?}", result.unwrap_err());
        eprintln!("Skipping these files");
        return Ok(res);
    }

    let name_a = a.as_ref().to_string_lossy();
    let name_b = b.as_ref().to_string_lossy();
    let result = result.unwrap();

    res.add_link(&name_a, &name_b, result);
    Ok(res)
}

pub fn compare_folder<P: AsRef<path::Path>, Q: AsRef<path::Path>>(a: &P, b: &Q) -> CompareOutput {
    let mut res: HashMap<String, Rc<RefCell<HashSet<String>>>> = HashMap::new();

    if a.as_ref().is_dir() {
        // println!("Comparing {} and {}", &a.as_ref().to_str().unwrap(), &b.as_ref().to_str().unwrap());
        for d in fs::read_dir(a).unwrap() {
            let entry = d.unwrap();
            let path = entry.path();

            let mut val = compare_folder(&path, b)?;
            for (name, set) in val.drain() {
                // println!("Same set: {:?}", &set);
                // println!("Same name: {:?}", &name);

                if !res.contains_key(&name) {
                    res.insert(name, set);
                    continue;
                }
                if set.borrow().len() > 0 {
                    // println!("Same set: {:?}", &set);
                    // println!("Same name: {:?}", &name);
                    let mut s = res.get(&name).unwrap().borrow_mut();
                    for pth in set.borrow().iter() {
                        s.insert(pth.clone());
                    }
                }
            }
        }

        return Ok(res);
    }

    if b.as_ref().is_dir() {
        return compare_folder(b, a);
    }

    let result = compare_file(a, b);
    if result.is_err() {
        eprintln!(
            "Cannot compare files {} and {}",
            a.as_ref().to_str().unwrap(),
            b.as_ref().to_str().unwrap()
        );
        eprintln!("Error msg: {:?}", result.unwrap_err());
        eprintln!("Skipping these files");
        return Ok(res);
    }

    let check = result.unwrap();
    let set: HashSet<String> = HashSet::new();
    let cell: RefCell<HashSet<String>> = RefCell::new(set);
    let name_a = a.as_ref().to_string_lossy().into_owned();
    let name_b = b.as_ref().to_string_lossy().into_owned();
    if check {
        let rset: Rc<RefCell<HashSet<String>>> = Rc::new(cell);
        rset.borrow_mut().insert(name_a.clone());
        rset.borrow_mut().insert(name_b.clone());
        res.insert(name_a, rset.clone());
        res.insert(name_b, rset);
    } else {
        res.insert(name_a, Rc::new(RefCell::new(HashSet::new())));
        res.insert(name_b, Rc::new(RefCell::new(HashSet::new())));
    }

    Ok(res)
}

pub fn print_comparison(c: CompareOutput) -> io::Result<String> {
    let mut missing: Vec<String> = Vec::new();
    let mut same: Vec<HashSet<String>> = Vec::new();
    let res = c.unwrap();
    // for (name, output) in &res {
    //     if output.borrow().len() != 0 {
    //         println!("Name: {:?}", &name);
    //         println!("Content: {:?}", &output);
    //         for o in output.borrow().iter() {
    //             println!("Opposite name: {:?}", o);
    //             println!("Opposite content: {:?}", &res.get(o));
    //         }
    //     }
    // }
    for (name, output) in &res {
        // println!("Name: {:?}", &name);
        // println!("Content: {:?}", &output);
        if output.borrow().len() == 0 {
            missing.push(name.clone());
            continue;
        }

        let mut add = true;
        for value in &same {
            let set = output.borrow();
            let sym: Vec<&String> = value.symmetric_difference(&set).collect();
            if sym.is_empty() {
                add = false;
                break;
            }
        }

        if add {
            let cell = &*output;
            let mut set: HashSet<String> = HashSet::new();
            for s in cell.borrow().iter() {
                set.insert(s.clone());
            }
            same.push(set);
            // for v in &same {
            //     for s in v.iter() {
            //         println!("Set: {:?}", &res.get(s).unwrap());
            //     }
            // }
        }

        // println!("Name: {:?}", &name);
        // println!("Content: {:?}", &output);
        // println!("Same: {:?}", &same);
    }

    missing.sort();
    same.sort_by(|x, y| x.len().cmp(&y.len()));
    let mut msg = format!("Comparison report\n");
    for _ in 0..msg.len() {
        msg.push('-');
    }

    msg.push('\n');
    msg.push_str("Same contents:\n");
    if same.is_empty() {
        msg.push_str("[]");
        msg.push('\n');
    } else {
        msg.push('[');
        msg.push('\n');
        for mut s in same {
            let mut v: Vec<String> = s.drain().collect::<Vec<String>>();
            v.sort();
            let set_str = v.iter().map(|k| &**k).collect::<Vec<&str>>().join(", ");
            msg.push_str("  ");
            msg.push_str(&set_str);
            msg.push(';');
            msg.push('\n');
        }
        msg.push(']');
        msg.push('\n');
    }

    msg.push_str("Missing:\n");
    if missing.is_empty() {
        msg.push('[');
        msg.push(']');
        msg.push('\n');
    } else {
        msg.push('[');
        msg.push('\n');
        for m in &missing {
            msg.push_str("  ");
            msg.push_str(&m);
            msg.push(';');
            msg.push('\n');
        }
        msg.push(']');
        msg.push('\n');
    }

    Ok(msg)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_link_files_basic() {
        let mut f = FileComparison::new("Left", "Right");
        f.add_left("LA");
        f.add_left("LB");
        f.add_right("RA");
        f.add_right("RZ");

        f.link_files("LA", "RA");
        f.link_files("LC", "RB");
        f.link_files("LB", "RC");
        println!("{:?}", f);
        f.print();

        let names = vec!["LA", "LB", "LC", "RA", "RB", "RC", "RZ"];
        for val in &names {
            let val = String::from(*val);
            assert_eq!(true, f.files.contains(&val))
        }
        for val in f.files.iter() {
            let s = val.as_str();
            assert_eq!(true, names.contains(&s));
        }

        for val in vec!["LA", "LB", "LC"] {
            let val = String::from(val);
            assert_eq!(true, f.left.contains_key(&val));
        }

        for val in vec!["RA", "RB", "RC"] {
            let val = String::from(val);
            assert_eq!(true, f.right.contains_key(&val));
        }

        for (lhs, rhs) in vec![("LA", "RA"), ("LC", "RB"), ("LB", "RC")] {
            let lhs = f.files.get(&String::from(lhs)).unwrap();
            let rhs = f.files.get(&String::from(rhs)).unwrap();
            let mut s: HashSet<Rc<String>> = HashSet::new();
            s.insert(lhs.clone());
            s.insert(rhs.clone());

            assert_eq!(true, f.links.contains(&s));
        }

    }

    #[test]
    fn test_from_link() {

        let mut link = FileLink::new();
        link.add_link("LA", "RA", true);
        link.add_link("LA", "RB", false);
        link.add_link("LC", "RB", true);
        link.add_link("LB", "RC", true);
        link.add_link("LA", "RZ", false);

        let mut f = FileComparison::new("Left", "Right");
        f.merge_link(&mut link);

        f.print();
        assert_eq!(1, 2);

    }
}
