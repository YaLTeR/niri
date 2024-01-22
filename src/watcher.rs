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
                    // this "should" be as simple as mtime, but it does not quite work in practice;
                    // it doesn't work if the config is a symlink, and its target changes but the
                    // new target and old target have identical mtimes.
                    //
                    // in practice, this does not occur on any systems other than nix.
                    // because, on nix practically everything is a symlink to /nix/store
                    // and due to reproducibility, /nix/store keeps no mtime (= 1970-01-01)
                    // so, symlink targets change frequently when mtime doesn't.
                    let mut last_props = path
                        .canonicalize()
                        .and_then(|canon| Ok((canon.metadata()?.modified()?, canon)))
                        .ok();

                    loop {
                        thread::sleep(Duration::from_millis(500));

                        if should_stop.load(Ordering::SeqCst) {
                            break;
                        }

                        if let Ok(new_props) = path
                            .canonicalize()
                            .and_then(|canon| Ok((canon.metadata()?.modified()?, canon)))
                        {
                            if last_props.as_ref() != Some(&new_props) {
                                trace!("file changed: {}", path.to_string_lossy());

                                if let Err(err) = changed.send(()) {
                                    warn!("error sending change notification: {err:?}");
                                    break;
                                }

                                last_props = Some(new_props);
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
