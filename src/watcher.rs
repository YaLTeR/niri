//! File modification watcher.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use smithay::reexports::calloop::channel::SyncSender;

pub struct Watcher {
    should_stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }
}

impl Watcher {
    pub fn new(path: PathBuf, changed: SyncSender<()>) -> Self {
        let should_stop = Arc::new(AtomicBool::new(false));

        {
            let should_stop = should_stop.clone();
            thread::Builder::new()
                .name(format!("Filesystem Watcher for {}", path.to_string_lossy()))
                .spawn(move || {
                    let mut last_mtime = path.metadata().and_then(|meta| meta.modified()).ok();

                    loop {
                        thread::sleep(Duration::from_millis(500));

                        if should_stop.load(Ordering::SeqCst) {
                            break;
                        }

                        if let Ok(mtime) = path.metadata().and_then(|meta| meta.modified()) {
                            if last_mtime != Some(mtime) {
                                trace!("file changed: {}", path.to_string_lossy());

                                if let Err(err) = changed.send(()) {
                                    warn!("error sending change notification: {err:?}");
                                    break;
                                }

                                last_mtime = Some(mtime);
                            }
                        }
                    }

                    debug!("exiting watcher thread for {}", path.to_string_lossy());
                })
                .unwrap();
        }

        Self { should_stop }
    }
}
