use std::collections::hash_map::Entry;

use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
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

use super::xdg_shell::add_mapped_toplevel_pre_commit_hook;
use crate::niri::{ClientState, State};
use crate::window::{InitialConfigureState, Mapped, ResolvedWindowRules, Unmapped};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.niri.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn new_subsurface(&mut self, surface: &WlSurface, parent: &WlSurface) {
        let mut root = parent.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }

        if let Some(output) = self.niri.output_for_root(&root) {
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
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        let _span = tracy_client::span!("CompositorHandler::commit");

        on_commit_buffer_handler::<Self>(surface);
        self.backend.early_import(surface);

        if is_sync_subsurface(surface) {
            return;
        }

        let mut root_surface = surface.clone();
        while let Some(parent) = get_parent(&root_surface) {
            root_surface = parent;
        }

        // Update the cached root surface.
        self.niri
            .root_surface
            .insert(surface.clone(), root_surface.clone());

        if surface == &root_surface {
            // This is a root surface commit. It might have mapped a previously-unmapped toplevel.
            if let Entry::Occupied(entry) = self.niri.unmapped_windows.entry(surface.clone()) {
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some())
                        .unwrap_or_else(|| {
                            error!("no renderer surface state even though we use commit handler");
                            false
                        });

                if is_mapped {
                    // The toplevel got mapped.
                    let Unmapped { window, state } = entry.remove();

                    window.on_commit();

                    let toplevel = window.toplevel().expect("no X11 support");

                    let (rules, width, is_full_width, output) =
                        if let InitialConfigureState::Configured {
                            rules,
                            width,
                            is_full_width,
                            output,
                        } = state
                        {
                            // Check that the output is still connected.
                            let output =
                                output.filter(|o| self.niri.layout.monitor_for_output(o).is_some());

                            (rules, width, is_full_width, output)
                        } else {
                            error!("window map must happen after initial configure");
                            (ResolvedWindowRules::empty(), None, false, None)
                        };

                    let parent = toplevel
                        .parent()
                        .and_then(|parent| self.niri.layout.find_window_and_output(&parent))
                        // Only consider the parent if we configured the window for the same
                        // output.
                        //
                        // Normally when we're following the parent, the configured output will be
                        // None. If the configured output is set, that means it was set explicitly
                        // by a window rule or a fullscreen request.
                        .filter(|(_, parent_output)| {
                            output.is_none() || output.as_ref() == Some(*parent_output)
                        })
                        .map(|(mapped, _)| mapped.window.clone());

                    let hook = add_mapped_toplevel_pre_commit_hook(toplevel);
                    let mapped = Mapped::new(window, rules, hook);
                    let window = mapped.window.clone();

                    let output = if let Some(p) = parent {
                        // Open dialogs immediately to the right of their parent window.
                        self.niri
                            .layout
                            .add_window_right_of(&p, mapped, width, is_full_width)
                    } else if let Some(output) = &output {
                        self.niri
                            .layout
                            .add_window_on_output(output, mapped, width, is_full_width);
                        Some(output)
                    } else {
                        self.niri.layout.add_window(mapped, width, is_full_width)
                    };

                    if let Some(output) = output.cloned() {
                        self.niri.layout.start_open_animation_for_window(&window);

                        let new_active_window =
                            self.niri.layout.active_window().map(|(m, _)| &m.window);
                        if new_active_window == Some(&window) {
                            self.maybe_warp_cursor_to_focus();
                        }

                        self.niri.queue_redraw(&output);
                    }
                    return;
                }

                // The toplevel remains unmapped.
                let unmapped = entry.get();
                if unmapped.needs_initial_configure() {
                    let toplevel = unmapped.window.toplevel().expect("no x11 support").clone();
                    self.queue_initial_configure(toplevel);
                }
                return;
            }

            // This is a commit of a previously-mapped root or a non-toplevel root.
            if let Some((mapped, output)) = self.niri.layout.find_window_and_output(surface) {
                let window = mapped.window.clone();
                let output = output.clone();

                // This is a commit of a previously-mapped toplevel.
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some())
                        .unwrap_or_else(|| {
                            error!("no renderer surface state even though we use commit handler");
                            false
                        });

                // Must start the close animation before window.on_commit().
                if !is_mapped {
                    self.backend.with_primary_renderer(|renderer| {
                        self.niri
                            .layout
                            .start_close_animation_for_window(renderer, &window);
                    });
                }

                window.on_commit();

                if !is_mapped {
                    // The toplevel got unmapped.
                    //
                    // Test client: wleird-unmap.
                    let active_window = self.niri.layout.active_window().map(|(m, _)| &m.window);
                    let was_active = active_window == Some(&window);

                    self.niri.layout.remove_window(&window);

                    if was_active {
                        self.maybe_warp_cursor_to_focus();
                    }

                    // Newly-unmapped toplevels must perform the initial commit-configure sequence
                    // afresh.
                    let unmapped = Unmapped::new(window);
                    self.niri.unmapped_windows.insert(surface.clone(), unmapped);

                    self.niri.queue_redraw(&output);
                    return;
                }

                // The toplevel remains mapped.
                self.niri.layout.update_window(&window);

                // Popup placement depends on window size which might have changed.
                self.update_reactive_popups(&window, &output);

                self.niri.queue_redraw(&output);
                return;
            }

            // This is a commit of a non-toplevel root.
        }

        // This is a commit of a non-root or a non-toplevel root.
        let root_window_output = self.niri.layout.find_window_and_output(&root_surface);
        if let Some((mapped, output)) = root_window_output {
            let window = mapped.window.clone();
            let output = output.clone();
            window.on_commit();
            self.niri.layout.update_window(&window);
            self.niri.queue_redraw(&output);
            return;
        }

        // This might be a popup.
        self.popups_handle_commit(surface);
        if let Some(popup) = self.niri.popups.find_popup(surface) {
            if let Some(output) = self.output_for_popup(&popup) {
                self.niri.queue_redraw(&output.clone());
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
                        self.niri.queue_redraw(&output.clone());
                        break;
                    }
                }
            }
        }
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        // Clients may destroy their subsurfaces before the main surface. Ensure we have a snapshot
        // when that happens, so that the closing animation includes all these subsurfaces.
        //
        // Test client: alacritty with CSD.
        if let Some(root) = self.niri.root_surface.get(surface) {
            if let Some((mapped, _)) = self.niri.layout.find_window_and_output(root) {
                self.backend.with_primary_renderer(|renderer| {
                    mapped.render_and_store_snapshot(renderer);
                });
            }
        }

        self.niri
            .root_surface
            .retain(|k, v| k != surface && v != surface);
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
