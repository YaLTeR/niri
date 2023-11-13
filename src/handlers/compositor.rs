use std::collections::hash_map::Entry;

use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::desktop::find_popup_root_surface;
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::calloop::Interest;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, send_surface_state,
    with_states, BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
    SurfaceAttributes,
};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{delegate_compositor, delegate_shm};

use super::xdg_shell;
use crate::niri::{ClientState, State};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.niri.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn new_subsurface(&mut self, surface: &WlSurface, parent: &WlSurface) {
        if let Some((_, output)) = self.niri.layout.find_window_and_output(parent) {
            let scale = output.current_scale().integer_scale();
            let transform = output.current_transform();
            with_states(surface, |data| {
                send_surface_state(surface, data, scale, transform);
            });
        }
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
            let maybe_dmabuf = with_states(surface, |surface_data| {
                surface_data
                    .cached_state
                    .pending::<SurfaceAttributes>()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).ok(),
                        _ => None,
                    })
            });
            if let Some(dmabuf) = maybe_dmabuf {
                if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                    let client = surface.client().unwrap();
                    let res = state
                        .niri
                        .event_loop
                        .insert_source(source, move |_, _, state| {
                            let display_handle = state.niri.display_handle.clone();
                            state
                                .client_compositor_state(&client)
                                .blocker_cleared(state, &display_handle);
                            Ok(())
                        });
                    if res.is_ok() {
                        add_blocker(surface, blocker);
                    }
                }
            }
        })
    }

    fn commit(&mut self, surface: &WlSurface) {
        let _span = tracy_client::span!("CompositorHandler::commit");

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
            if let Entry::Occupied(entry) = self.niri.unmapped_windows.entry(surface.clone()) {
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some());

                if is_mapped {
                    // The toplevel got mapped.
                    let window = entry.remove();
                    window.on_commit();

                    if let Some(output) = self
                        .niri
                        .layout
                        .add_window(window, true, None, false)
                        .cloned()
                    {
                        self.niri.queue_redraw(output);
                    }
                    return;
                }

                // The toplevel remains unmapped.
                let window = entry.get();
                xdg_shell::send_initial_configure_if_needed(window.toplevel());
                return;
            }

            // This is a commit of a previously-mapped root or a non-toplevel root.
            if let Some((window, output)) = self.niri.layout.find_window_and_output(surface) {
                // This is a commit of a previously-mapped toplevel.
                window.on_commit();

                // This is a commit of a previously-mapped toplevel.
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some());

                if !is_mapped {
                    // The toplevel got unmapped.
                    self.niri.layout.remove_window(&window);
                    self.niri.unmapped_windows.insert(surface.clone(), window);
                    self.niri.queue_redraw(output);
                    return;
                }

                // The toplevel remains mapped.
                self.niri.layout.update_window(&window);

                self.niri.queue_redraw(output);
                return;
            }

            // This is a commit of a non-toplevel root.
        }

        // This is a commit of a non-root or a non-toplevel root.
        let root_window_output = self.niri.layout.find_window_and_output(&root_surface);
        if let Some((window, output)) = root_window_output {
            window.on_commit();
            self.niri.layout.update_window(&window);
            self.niri.queue_redraw(output);
            return;
        }

        // This might be a popup.
        self.popups_handle_commit(surface);
        if let Some(popup) = self.niri.popups.find_popup(surface) {
            if let Ok(root) = find_popup_root_surface(&popup) {
                let root_window_output = self.niri.layout.find_window_and_output(&root);
                if let Some((_window, output)) = root_window_output {
                    self.niri.queue_redraw(output);
                }
            }
        }

        // This might be a layer-shell surface.
        self.layer_shell_handle_commit(surface);

        // This might be a cursor surface.
        if matches!(&self.niri.cursor_manager.cursor_image(), CursorImageStatus::Surface(s) if s == surface)
        {
            // FIXME: granular redraws for cursors.
            self.niri.queue_redraw_all();
        }

        // This might be a DnD icon surface.
        if self.niri.dnd_icon.as_ref() == Some(surface) {
            // FIXME: granular redraws for cursors.
            self.niri.queue_redraw_all();
        }

        // This might be a lock surface.
        if self.niri.is_locked() {
            for (output, state) in &self.niri.output_state {
                if let Some(lock_surface) = &state.lock_surface {
                    if lock_surface.wl_surface() == surface {
                        self.niri.queue_redraw(output.clone());
                        break;
                    }
                }
            }
        }
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.niri.shm_state
    }
}

delegate_compositor!(State);
delegate_shm!(State);
