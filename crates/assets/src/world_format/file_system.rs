use std::io::{self, Read};
use std::path::Path;

pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_prefix(&self, path: &Path, max_len: usize) -> io::Result<Vec<u8>> {
        let mut data = self.read(path)?;
        data.truncate(max_len);
        Ok(data)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_prefix(&self, path: &Path, max_len: usize) -> io::Result<Vec<u8>> {
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::with_capacity(max_len);
        file.take(max_len as u64).read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        std::fs::write(path, data)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}
