//! A pipe read with a deadline: another app writes the far end (its copy,
//! sent through the compositor), and an app that never closes it must not
//! keep a thread of ours blocked for good.

use std::{
    io::{ErrorKind, PipeReader, Read},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

/// Everything until the writer closes; None: `within` ran out first.
pub fn read_within(from: &mut PipeReader, within: Duration) -> Option<Vec<u8>> {
    let end = Instant::now() + within;
    let (mut out, mut buf) = (Vec::new(), [0u8; 8192]);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        if !readable(from, left)? {
            continue; // the time ran out (seen above next round), or a signal
        }
        match from.read(&mut buf) {
            Ok(0) => return Some(out),
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// Something to read (or the writer closed) within `left`; Some(false): not
/// yet (the time ran out, or a signal came first); None: poll failed.
fn readable(from: &PipeReader, left: Duration) -> Option<bool> {
    let mut fd = libc::pollfd {
        fd: from.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = i32::try_from(left.as_millis()).unwrap_or(i32::MAX).max(1);
    // SAFETY: one pollfd, valid for the whole call; the fd stays open
    // (borrowed from `from`) until it returns.
    let n = unsafe { libc::poll(&raw mut fd, 1, ms) };
    match n {
        0 => Some(false),
        n if n > 0 => Some(true),
        _ if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::read_within;
    use std::{io::Write, time::Duration};

    #[test]
    fn it_reads_to_the_close_and_gives_up_on_a_writer_that_stays() {
        let (mut from, mut to) = std::io::pipe().expect("a pipe");
        to.write_all(b"hello").expect("written");
        drop(to);
        let all = read_within(&mut from, Duration::from_secs(1));
        assert_eq!(all.as_deref(), Some(&b"hello"[..]));
        let (mut from, mut to) = std::io::pipe().expect("a pipe");
        to.write_all(b"half").expect("written"); // and never closed
        assert_eq!(read_within(&mut from, Duration::from_millis(100)), None);
        drop(to);
    }
}
