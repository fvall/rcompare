use crate::file::{walk_dir, FileInfo};
use fasthash::{city, RandomState};
use serde::ser::SerializeStruct;
use serde::Serialize;
use std::collections::HashMap;
use std::io;
use std::path;

pub(crate) type VecIdx = Vec<usize>;

#[derive(Debug, Default, Clone)]
pub struct FileSeparation {
    pub same: Vec<VecIdx>,
    pub unique: VecIdx,
    pub errors: VecIdx,
}

#[derive(Debug, Default, Clone)]
pub struct Preprocessed {
    pub info: Vec<FileInfo>,
    pub zero: VecIdx,
    pub unique: VecIdx,
    pub same: Vec<VecIdx>,
    pub to_process: Vec<VecIdx>,
}

#[derive(Debug, Default, Clone)]
pub struct Processed {
    // pub info: std::sync::Arc<Vec<FileInfo>>,
    pub info: Vec<FileInfo>,
    pub zero: VecIdx,
    pub unique: VecIdx,
    pub same: Vec<VecIdx>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ProcessedSerializationError {
    Message(String),
    IndexError(usize),
}

impl serde::ser::Error for ProcessedSerializationError {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        ProcessedSerializationError::Message(msg.to_string())
    }
}

impl std::error::Error for ProcessedSerializationError {}
impl std::fmt::Display for ProcessedSerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(msg) => f.write_str(msg),
            Self::IndexError(u) => f.write_str(&format!("Incorrect index at position {}", u)),
        }
    }
}

impl Serialize for Processed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Processed", 3)?;
        let mut same: Vec<Vec<&FileInfo>> = Vec::with_capacity(self.same.len());
        for v in self.same.iter() {
            let inner = map_to_file_info(&v, &self.info).map_err(serde::ser::Error::custom)?;
            same.push(inner);
        }

        let zero = map_to_file_info(&self.zero, &self.info).map_err(serde::ser::Error::custom)?;
        let unique = map_to_file_info(&self.unique, &self.info).map_err(serde::ser::Error::custom)?;

        state.serialize_field("zero", &zero)?;
        state.serialize_field("unique", &unique)?;
        state.serialize_field("same", &same)?;
        state.end()
    }
}

pub fn preprocess<P, Q>(lhs: Option<&P>, rhs: Option<&Q>) -> io::Result<Preprocessed>
where
    P: AsRef<path::Path>,
    Q: AsRef<path::Path>,
{
    let lpath = resolve_path(&lhs);
    if let Err(err) = lpath {
        eprintln!("Unable to resolve path {:?} for preprocessing", lhs.map(|x| x.as_ref()));
        return Err(err);
    }

    let lpath = lpath.unwrap();
    let rpath: &path::Path;
    let rpath_buf: path::PathBuf;
    if rhs.is_none() {
        rpath = lpath.as_path();
    } else {
        let rpath_buf_res = resolve_path(&rhs);
        if let Err(err) = rpath_buf_res {
            eprintln!("Unable to resolve path {:?} for preprocessing", rhs.map(|x| x.as_ref()));
            return Err(err);
        }

        rpath_buf = rpath_buf_res.unwrap();
        rpath = rpath_buf.as_path();
    }

    let iter_lhs = walk_dir(&lpath);
    let iter_rhs = (lpath.as_path() != rpath)
        .then_some(walk_dir(&rpath))
        .into_iter()
        .flatten();

    let mut unique: VecIdx = vec![];
    let mut zero_size: VecIdx = vec![];
    let mut size_map: HashMap<u64, VecIdx, RandomState<city::Hash64>> =
        HashMap::with_hasher(RandomState::<city::Hash64>::new());
    let mut contents: Vec<FileInfo> = vec![];

    let iter_dir = iter_lhs.chain(iter_rhs);
    for (idx, value) in iter_dir.enumerate() {
        contents.push(value);
        let value = contents.last().unwrap();
        if value.size == 0 {
            zero_size.push(idx);
            continue;
        }
        let entry = size_map.entry(value.size);
        entry.or_default().push(idx);
    }

    let same: Vec<VecIdx> = vec![];
    let mut to_be_processed = same.clone();

    for (_, mut value) in size_map.drain() {
        // if the sizes are different the files cannot be the same
        if value.len() == 1 {
            unique.push(value.pop().unwrap());
            continue;
        }

        if !value.is_empty() {
            to_be_processed.push(value);
        }
    }

    let prep = Preprocessed {
        info: contents,
        zero: zero_size,
        same,
        unique,
        to_process: to_be_processed,
    };

    Ok(prep)
}

// ----------
//  Internal
// ----------

fn resolve_path<P>(path: &Option<&P>) -> io::Result<path::PathBuf>
where
    P: AsRef<path::Path>,
{
    if let &Some(p) = path {
        let path_buf = std::fs::canonicalize(p.as_ref())?;
        return Ok(path_buf);
    }

    let cur = std::env::current_dir()?;
    let path_buf = std::fs::canonicalize(cur)?;
    Ok(path_buf)
}

fn map_to_file_info<'f>(v: &[usize], info: &'f [FileInfo]) -> Result<Vec<&'f FileInfo>, ProcessedSerializationError> {
    let mut inner: Vec<&FileInfo> = Vec::with_capacity(v.len());
    for idx in v.iter() {
        let file_info = info.get(*idx).ok_or(ProcessedSerializationError::IndexError(*idx))?;
        inner.push(file_info);
    }

    Ok(inner)
}
