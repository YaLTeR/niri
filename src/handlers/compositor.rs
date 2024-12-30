use std::collections::hash_map::Entry;

use niri_ipc::PositionChange;
use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::reexports::calloop::Interest;
use smithay::reexports::wayland_server::protocol::wl_buffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, remove_pre_commit_hook,
    with_states, BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
    SurfaceAttributes,
};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{delegate_compositor, delegate_shm};

use super::xdg_shell::add_mapped_toplevel_pre_commit_hook;
use crate::handlers::XDG_ACTIVATION_TOKEN_TIMEOUT;
use crate::layout::{ActivateWindow, AddWindowTarget};
use crate::niri::{ClientState, State};
use crate::utils::send_scale_transform;
use crate::utils::transaction::Transaction;
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
            let scale = output.current_scale();
            let transform = output.current_transform();
            with_states(surface, |data| {
                send_scale_transform(surface, data, scale, transform);
            });
        }
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        self.add_default_dmabuf_pre_commit_hook(surface);
    }

    fn commit(&mut self, surface: &WlSurface) {
        let _span = tracy_client::span!("CompositorHandler::commit");
        trace!(surface = ?surface.id(), "commit");

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
                    let Unmapped {
                        window,
                        state,
                        activation_token_data,
                    } = entry.remove();

                    window.on_commit();

                    let toplevel = window.toplevel().expect("no X11 support");

                    let (rules, width, height, is_full_width, output, workspace_id) =
                        if let InitialConfigureState::Configured {
                            rules,
                            width,
                            height,
                            floating_width: _,
                            floating_height: _,
                            is_full_width,
                            output,
                            workspace_name,
                        } = state
                        {
                            // Check that the output is still connected.
                            let output =
                                output.filter(|o| self.niri.layout.monitor_for_output(o).is_some());

                            // Check that the workspace still exists.
                            let workspace_id = workspace_name
                                .as_deref()
                                .and_then(|n| self.niri.layout.find_workspace_by_name(n))
                                .map(|(_, ws)| ws.id());

                            (rules, width, height, is_full_width, output, workspace_id)
                        } else {
                            error!("window map must happen after initial configure");
                            (ResolvedWindowRules::empty(), None, None, false, None, None)
                        };

                    // The GTK about dialog sets min/max size after the initial configure but
                    // before mapping, so we need to compute open_floating at the last possible
                    // moment, that is here.
                    let is_floating = rules.compute_open_floating(toplevel);

                    // Figure out if we should activate the window.
                    let activate = rules.open_focused.map(|focus| {
                        if focus {
                            ActivateWindow::Yes
                        } else {
                            ActivateWindow::No
                        }
                    });
                    let activate = activate.unwrap_or_else(|| {
                        // Check the token timestamp again in case the window took a while between
                        // requesting activation and mapping.
                        let token = activation_token_data.filter(|token| {
                            token.timestamp.elapsed() < XDG_ACTIVATION_TOKEN_TIMEOUT
                        });
                        if token.is_some() {
                            ActivateWindow::Yes
                        } else {
                            let config = self.niri.config.borrow();
                            if config.debug.strict_new_window_focus_policy {
                                ActivateWindow::No
                            } else {
                                ActivateWindow::Smart
                            }
                        }
                    });

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

                    // The mapped pre-commit hook deals with dma-bufs on its own.
                    self.remove_default_dmabuf_pre_commit_hook(toplevel.wl_surface());
                    let hook = add_mapped_toplevel_pre_commit_hook(toplevel);
                    let mapped = Mapped::new(window, rules, hook);
                    let window = mapped.window.clone();

                    let target = if let Some(p) = &parent {
                        // Open dialogs next to their parent window.
                        AddWindowTarget::NextTo(p)
                    } else if let Some(id) = workspace_id {
                        AddWindowTarget::Workspace(id)
                    } else if let Some(output) = &output {
                        AddWindowTarget::Output(output)
                    } else {
                        AddWindowTarget::Auto
                    };
                    let output = self.niri.layout.add_window(
                        mapped,
                        target,
                        width,
                        height,
                        is_full_width,
                        is_floating,
                        activate,
                    );

                    if let Some(output) = output.cloned() {
                        self.niri.layout.start_open_animation_for_window(&window);

                        let new_focus = self.niri.layout.focus().map(|m| &m.window);
                        if new_focus == Some(&window) {
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

                #[cfg(feature = "xdp-gnome-screencast")]
                let id = mapped.id();

                // This is a commit of a previously-mapped toplevel.
                let is_mapped =
                    with_renderer_surface_state(surface, |state| state.buffer().is_some())
                        .unwrap_or_else(|| {
                            error!("no renderer surface state even though we use commit handler");
                            false
                        });

                // Must start the close animation before window.on_commit().
                let transaction = Transaction::new();
                if !is_mapped {
                    let blocker = transaction.blocker();
                    self.backend.with_primary_renderer(|renderer| {
                        self.niri
                            .layout
                            .start_close_animation_for_window(renderer, &window, blocker);
                    });
                }

                window.on_commit();

                if !is_mapped {
                    // The toplevel got unmapped.
                    //
                    // Test client: wleird-unmap.
                    let active_window = self.niri.layout.focus().map(|m| &m.window);
                    let was_active = active_window == Some(&window);

                    #[cfg(feature = "xdp-gnome-screencast")]
                    self.niri
                        .stop_casts_for_target(crate::pw_utils::CastTarget::Window {
                            id: id.get(),
                        });

                    self.niri.layout.remove_window(&window, transaction.clone());
                    self.add_default_dmabuf_pre_commit_hook(surface);

                    // If this is the only instance, then this transaction will complete
                    // immediately, so no need to set the timer.
                    if !transaction.is_last() {
                        transaction.register_deadline_timer(&self.niri.event_loop);
                    }

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

                let (serial, buffer_delta) = with_states(surface, |states| {
                    let buffer_delta = states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .buffer_delta
                        .take();

                    let role = states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap();
                    (role.configure_serial, buffer_delta)
                });
                if serial.is_none() {
                    error!("commit on a mapped surface without a configured serial");
                }

                // The toplevel remains mapped.
                self.niri.layout.update_window(&window, serial);

                // Move the toplevel according to the attach offset.
                if let Some(delta) = buffer_delta {
                    if delta.x != 0 || delta.y != 0 {
                        let (x, y) = delta.to_f64().into();
                        self.niri.layout.move_floating_window(
                            Some(&window),
                            PositionChange::AdjustFixed(x),
                            PositionChange::AdjustFixed(y),
                            false,
                        );
                    }
                }

                // Popup placement depends on window size which might have changed.
                self.update_reactive_popups(&window);

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
            self.niri.layout.update_window(&window, None);
            self.niri.queue_redraw(&output);
            return;
        }

        // This might be a popup.
        self.popups_handle_commit(surface);
        if let Some(popup) = self.niri.popups.find_popup(surface) {
            if let Some(output) = self.output_for_popup(&popup) {
                self.niri.queue_redraw(&output.clone());
            }
            return;
        }

        // This might be a layer-shell surface.
        if self.layer_shell_handle_commit(surface) {
            return;
        }

        // This might be a cursor surface.
        if matches!(
            &self.niri.cursor_manager.cursor_image(),
            CursorImageStatus::Surface(s) if s == &root_surface
        ) {
            // In case the cursor surface has been committed handle the role specific
            // buffer offset by applying the offset on the cursor image hotspot
            if surface == &root_surface {
                with_states(surface, |states| {
                    let cursor_image_attributes = states.data_map.get::<CursorImageSurfaceData>();

                    if let Some(mut cursor_image_attributes) =
                        cursor_image_attributes.map(|attrs| attrs.lock().unwrap())
                    {
                        let buffer_delta = states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .buffer_delta
                            .take();
                        if let Some(buffer_delta) = buffer_delta {
                            cursor_image_attributes.hotspot -= buffer_delta;
                        }
                    }
                });
            }

            // FIXME: granular redraws for cursors.
            self.niri.queue_redraw_all();
            return;
        }

        // This might be a DnD icon surface.
        if matches!(&self.niri.dnd_icon, Some(icon) if icon.surface == root_surface) {
            let dnd_icon = self.niri.dnd_icon.as_mut().unwrap();

            // In case the dnd surface has been committed handle the role specific
            // buffer offset by applying the offset on the dnd icon offset
            if surface == &dnd_icon.surface {
                with_states(&dnd_icon.surface, |states| {
                    let buffer_delta = states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .buffer_delta
                        .take()
                        .unwrap_or_default();
                    dnd_icon.offset += buffer_delta;
                });
            }

            // FIXME: granular redraws for cursors.
            self.niri.queue_redraw_all();
            return;
        }

        // This might be a lock surface.
        if self.niri.is_locked() {
            for (output, state) in &self.niri.output_state {
                if let Some(lock_surface) = &state.lock_surface {
                    if lock_surface.wl_surface() == &root_surface {
                        self.niri.queue_redraw(&output.clone());
                        return;
                    }
                }
            }
        }
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        // Clients may destroy their subsurfaces before the main surface. Ensure we have a snapshot
        // when that happens, so that the closing animation includes all these subsurfaces.
        //
        // Test client: alacritty with CSD <= 0.13 (it was fixed in winit afterwards:
        // https://github.com/rust-windowing/winit/pull/3625).
        //
        // This is still not perfect, as this function is called already after the (first)
        // subsurface is destroyed; in the case of alacritty, this is the top CSD shadow. But, it
        // gets most of the job done.
        if let Some(root) = self.niri.root_surface.get(surface) {
            if let Some((mapped, _)) = self.niri.layout.find_window_and_output(root) {
                let window = mapped.window.clone();
                self.backend.with_primary_renderer(|renderer| {
                    self.niri.layout.store_unmap_snapshot(renderer, &window);
                });
            }
        }

        self.niri
            .root_surface
            .retain(|k, v| k != surface && v != surface);

        self.niri.dmabuf_pre_commit_hook.remove(surface);
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

impl State {
    pub fn add_default_dmabuf_pre_commit_hook(&mut self, surface: &WlSurface) {
        let hook = add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
            let maybe_dmabuf = with_states(surface, |surface_data| {
                surface_data
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .pending()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                        _ => None,
                    })
            });
            if let Some(dmabuf) = maybe_dmabuf {
                if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                    if let Some(client) = surface.client() {
                        let res =
                            state
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
                            trace!("added default dmabuf blocker");
                        }
                    }
                }
            }
        });

        let s = surface.clone();
        if let Some(prev) = self.niri.dmabuf_pre_commit_hook.insert(s, hook) {
            error!("tried to add dmabuf pre-commit hook when there was already one");
            remove_pre_commit_hook(surface, prev);
        }
    }

    pub fn remove_default_dmabuf_pre_commit_hook(&mut self, surface: &WlSurface) {
        if let Some(hook) = self.niri.dmabuf_pre_commit_hook.remove(surface) {
            remove_pre_commit_hook(surface, hook);
        } else {
            error!("tried to remove dmabuf pre-commit hook but there was none");
        }
    }
}
