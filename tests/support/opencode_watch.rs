//! Existing close-write observation shared by synthetic OpenCode wrapper tests.
use std::{
    ffi::CString,
    fs::File,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    sync::mpsc,
    thread,
};

pub fn watch(directory: &Path) -> mpsc::Receiver<std::io::Result<()>> {
    let descriptor = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    assert!(descriptor >= 0);
    let file = unsafe { File::from_raw_fd(descriptor) };
    let directory = CString::new(directory.as_os_str().as_bytes()).unwrap();
    let watch = unsafe {
        libc::inotify_add_watch(file.as_raw_fd(), directory.as_ptr(), libc::IN_CLOSE_WRITE)
    };
    assert!(watch >= 0);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(closed_endpoint(file));
    });
    rx
}
fn closed_endpoint(mut file: File) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    loop {
        let size = file.read(&mut buffer)?;
        if contains_endpoint(&buffer[..size]) {
            return Ok(());
        }
    }
}
fn contains_endpoint(bytes: &[u8]) -> bool {
    let mut offset = 0;
    while offset + std::mem::size_of::<libc::inotify_event>() <= bytes.len() {
        let event = unsafe {
            std::ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<libc::inotify_event>())
        };
        offset += std::mem::size_of::<libc::inotify_event>();
        let name = &bytes[offset..offset + event.len as usize];
        if name.split(|b| *b == 0).next() == Some(b"opencode-endpoint.json".as_slice()) {
            return true;
        }
        offset += event.len as usize;
    }
    false
}
