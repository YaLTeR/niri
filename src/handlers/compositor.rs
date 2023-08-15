use std::collections::hash_map::Entry;

use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::desktop::find_popup_root_surface;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{delegate_compositor, delegate_shm};

use super::xdg_shell;
use crate::grabs::resize_grab;
use crate::niri::ClientState;
use crate::Niri;

impl CompositorHandler for Niri {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        tracy_client::Client::running()
            .unwrap()
            .message("client commit", 0);

        on_commit_buffer_handler::<Self>(surface);

        if is_sync_subsurface(surface) {
            return;
        }

        let mut root_surface = surface.clone();
        while let Some(parent) = get_parent(&root_surface) {
            root_surface = parent;
        }

        if surface == &root_surface {
            // This is a root surface commit. It might have mapped a previously-unmapped toplevel.
            if let Entry::Occupied(entry) = self.unmapped_windows.entry(surface.clone()) {
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some());

                if is_mapped {
                    // The toplevel got mapped.
                    let window = entry.remove();
                    window.on_commit();

                    let output = self.monitor_set.active_output().unwrap().clone();
                    self.monitor_set.add_window_to_output(&output, window, true);
                    self.queue_redraw(output);
                    return;
                }

                // The toplevel remains unmapped.
                let window = entry.get();
                xdg_shell::send_initial_configure_if_needed(window);
                return;
            }

            // This is a commit of a previously-mapped root or a non-toplevel root.
            if let Some((window, output)) = self.monitor_set.find_window_and_output(surface) {
                // This is a commit of a previously-mapped toplevel.
                window.on_commit();

                // This is a commit of a previously-mapped toplevel.
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some());

                if !is_mapped {
                    // The toplevel got unmapped.
                    self.monitor_set.remove_window(&window);
                    self.unmapped_windows.insert(surface.clone(), window);
                    self.queue_redraw(output);
                    return;
                }

                // The toplevel remains mapped.
                resize_grab::handle_commit(&window);
                self.monitor_set.update_window(&window);

                self.queue_redraw(output);
                return;
            }

            // This is a commit of a non-toplevel root.
        }

        // This is a commit of a non-root or a non-toplevel root.
        let root_window_output = self.monitor_set.find_window_and_output(&root_surface);
        if let Some((window, output)) = root_window_output {
            window.on_commit();
            self.monitor_set.update_window(&window);
            self.queue_redraw(output);
            return;
        }

        // This might be a popup.
        self.popups_handle_commit(surface);
        if let Some(popup) = self.popups.find_popup(surface) {
            if let Ok(root) = find_popup_root_surface(&popup) {
                let root_window_output = self.monitor_set.find_window_and_output(&root);
                if let Some((_window, output)) = root_window_output {
                    self.queue_redraw(output);
                }
            }
        }
    }
}

impl BufferHandler for Niri {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Niri {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(Niri);
delegate_shm!(Niri);
