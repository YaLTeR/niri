fn lock(&mut self) {
    // Prevent locking if input is currently inhibited.
    if self.niri.input_inhibited {
        debug!("Screen lock prevented because input is inhibited");
        return;
    }

    if self.niri.is_locked() {
        return;
    }
} 