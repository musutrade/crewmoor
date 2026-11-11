use crate::Result;

/// Linux MAX_ARG_STRLEN is PAGE_SIZE * 32, including the terminating NUL.
/// The TUI CLI passes the complete serialized envelope as a single argument.
pub(super) fn linux_argument(text: &str) -> Result<()> {
    // SAFETY: sysconf reads a system constant and has no pointer arguments.
    let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })?;
    let limit = page * 32;
    if text.len() >= limit {
        return Err(format!(
            "serialized message exceeds Linux single-argument limit ({limit} bytes including NUL); nothing submitted"
        ).into());
    }
    Ok(())
}
