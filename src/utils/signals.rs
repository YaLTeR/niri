//! We set a signal handler with `calloop::signals::Signals::new`.
//! This does two things:
//! 1. It blocks the thread from receiving these signals normally (pthread_sigmask)
//! 2. It creates a signalfd to read them in the event loop.
//!
//! When spawning children, calloop already deals with the signalfd.
//! `Signals::new` creates it with CLOEXEC, so it will not be inherited by children.
//!
//! But, the sigmask is always inherited, so we want to clear it before spawning children.
//! That way, we don't affect their normal signal handling.
//!
//! In particular, if a child doesn't care about signals, we must not block it from receiving them.
//!
//! This module provides functions to clear the sigmask. Call them before spawning children.
//!
//! Technically, a "more correct" solution would be to remember the original sigmask and restore it
//! after the child exits, but that's painful *and* likely to cause issues, because the user almost
//! never intended to spawn niri with a nonempty sigmask. It indicates a bug in whoever spawned us,
//! so we may as well clean up after them (which is easier than not doing so).

use std::{io, mem};

use calloop::signals::{Signal, Signals};

pub fn listen(handle: &calloop::LoopHandle<crate::niri::State>) {
    handle
        .insert_source(
            Signals::new(&[Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP]).unwrap(),
            |event, _, state| {
                info!("quitting due to receiving signal {:?}", event.signal());
                state.niri.stop_signal.stop();
            },
        )
        .unwrap();
}

pub fn unblock_all() -> io::Result<()> {
    set_sigmask(&empty_sigset()?)
}

pub fn empty_sigset() -> io::Result<libc::sigset_t> {
    let mut sigset = mem::MaybeUninit::uninit();
    if unsafe { libc::sigemptyset(sigset.as_mut_ptr()) } == 0 {
        Ok(unsafe { sigset.assume_init() })
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn set_sigmask(set: &libc::sigset_t) -> io::Result<()> {
    let oldset = std::ptr::null_mut(); // ignore old mask
    if unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, set, oldset) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
