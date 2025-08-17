// SCHED_RESET_ON_FORK only exists on Linux.
#[cfg(target_os = "linux")]
pub fn set_rt_scheduling() {
    let res = unsafe {
        // Work around libc crate exposing more fields on musl.
        let mut param: libc::sched_param = std::mem::zeroed();
        // Set SCHED_RESET_ON_FORK and request minimal realtime round-robin prio for main pid.
        param.sched_priority = libc::sched_get_priority_min(libc::SCHED_RR);

        libc::pthread_setschedparam(
            libc::pthread_self(),
            libc::SCHED_RR | libc::SCHED_RESET_ON_FORK,
            &param,
        )
    };

    match res {
        libc::EPERM => debug!("no permission to set real-time policy"),
        libc::EINVAL => debug!("real-time policy not recognized or scheduling params wrong"),
        libc::ESRCH => debug!("thread ID not found for real-time policy"),
        0 => (),
        _ => warn!("unknown failure setting real-time policy: {res}"),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_rt_scheduling() {
    ()
}
