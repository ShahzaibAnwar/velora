//! The on-disk partial file. Writes are positional so parallel connections can fill different regions.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::Arc;

/// Test hook: called with the write offset before every write; returning an error simulates a disk
/// failure such as a full disk.
pub type WriteFault = Arc<dyn Fn(u64) -> io::Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct PartFile {
    file: Arc<File>,
    fault: Option<WriteFault>,
}

impl PartFile {
    /// Open (or create) the part file. With `fresh` the file is truncated, marked sparse (NTFS) and,
    /// if the size is known, extended to its final length so every segment can write in place.
    pub fn open(path: &Path, size: Option<u64>, fresh: bool, fault: Option<WriteFault>) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(fresh).open(path)?;
        if fresh {
            if let Some(size) = size {
                if size > 0 {
                    make_sparse(&file);
                }
                file.set_len(size)?;
            }
        }
        Ok(Self { file: Arc::new(file), fault })
    }

    /// Write all of `data` at `offset` on a blocking thread. The buffer is returned for reuse.
    pub async fn write_at(&self, offset: u64, data: Vec<u8>) -> (Vec<u8>, io::Result<()>) {
        let file = self.file.clone();
        let fault = self.fault.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let res = match fault.as_ref().map(|f| f(offset)) {
                Some(Err(e)) => Err(e),
                _ => write_all_at(&file, &data, offset),
            };
            (data, res)
        })
        .await;
        match joined {
            Ok(v) => v,
            Err(e) => (Vec::new(), Err(io::Error::other(e))),
        }
    }

    pub async fn sync(&self) -> io::Result<()> {
        let file = self.file.clone();
        tokio::task::spawn_blocking(move || file.sync_data()).await.map_err(io::Error::other)?
    }

    pub async fn truncate(&self) -> io::Result<()> {
        let file = self.file.clone();
        tokio::task::spawn_blocking(move || file.set_len(0)).await.map_err(io::Error::other)?
    }
}

#[cfg(windows)]
fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_write(buf, offset) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer")),
            Ok(n) => {
                buf = &buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(buf, offset)
}

/// Mark the file sparse so that writing near the end of a large preallocated file does not make NTFS
/// zero-fill everything before it first. Failure (e.g. FAT32/exFAT) is harmless.
#[cfg(windows)]
fn make_sparse(file: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let mut returned = 0u32;
    // SAFETY: the handle is valid for the lifetime of `file`; no buffers are passed.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        tracing::debug!("could not mark part file sparse: {}", io::Error::last_os_error());
    }
}

#[cfg(not(windows))]
fn make_sparse(_file: &File) {}

/// Free space available to the current user on the volume containing `path`.
#[cfg(windows)]
pub fn available_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated; out pointers are valid or null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn available_space(_path: &Path) -> Option<u64> {
    None
}
