//! A kernel file held open and re-read from offset zero on every sample.
//!
//! procfs and sysfs regenerate a file's contents on a read at offset zero, so seeking back
//! replaces an open and a close per sample with one `lseek`. The buffer is reused, so a
//! steady-state read allocates nothing.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// One open kernel file and its read buffer.
#[derive(Debug)]
pub struct Source {
    path: PathBuf,
    file: File,
    buf: String,
}

/// A read that failed, with the path it failed on.
#[derive(Debug)]
pub struct ReadError {
    /// The file.
    pub path: PathBuf,
    /// The OS error.
    pub error: std::io::Error,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.error)
    }
}

impl Source {
    /// Open `path`. Called at startup, so a missing source fails before collection begins.
    pub fn open(path: impl AsRef<Path>) -> Result<Source, ReadError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|error| ReadError {
            path: path.clone(),
            error,
        })?;
        Ok(Source {
            path,
            file,
            buf: String::with_capacity(4096),
        })
    }

    /// Read the whole file again.
    pub fn read(&mut self) -> Result<&str, ReadError> {
        self.buf.clear();
        let result = self
            .file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.read_to_string(&mut self.buf));
        match result {
            Ok(_) => Ok(&self.buf),
            Err(error) => Err(ReadError {
                path: self.path.clone(),
                error,
            }),
        }
    }

    /// The path, for error messages.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
