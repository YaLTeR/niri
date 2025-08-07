use std::path::PathBuf;

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct DebugConfig {
    #[knuffel(child, unwrap(argument))]
    pub preview_render: Option<PreviewRender>,
    #[knuffel(child)]
    pub dbus_interfaces_in_non_session_instances: bool,
    #[knuffel(child)]
    pub wait_for_frame_completion_before_queueing: bool,
    #[knuffel(child)]
    pub enable_overlay_planes: bool,
    #[knuffel(child)]
    pub disable_cursor_plane: bool,
    #[knuffel(child)]
    pub disable_direct_scanout: bool,
    #[knuffel(child)]
    pub restrict_primary_scanout_to_matching_format: bool,
    #[knuffel(child, unwrap(argument))]
    pub render_drm_device: Option<PathBuf>,
    #[knuffel(child)]
    pub force_pipewire_invalid_modifier: bool,
    #[knuffel(child)]
    pub emulate_zero_presentation_time: bool,
    #[knuffel(child)]
    pub disable_resize_throttling: bool,
    #[knuffel(child)]
    pub disable_transactions: bool,
    #[knuffel(child)]
    pub keep_laptop_panel_on_when_lid_is_closed: bool,
    #[knuffel(child)]
    pub disable_monitor_names: bool,
    #[knuffel(child)]
    pub strict_new_window_focus_policy: bool,
    #[knuffel(child)]
    pub honor_xdg_activation_with_invalid_serial: bool,
    #[knuffel(child)]
    pub deactivate_unfocused_windows: bool,
    #[knuffel(child)]
    pub skip_cursor_only_updates_during_vrr: bool,
}

impl DebugConfig {
    pub fn merge_with(&mut self, other: &Self) {
        if other.preview_render.is_some() {
            self.preview_render = other.preview_render;
        }
        if other.dbus_interfaces_in_non_session_instances {
            self.dbus_interfaces_in_non_session_instances = true;
        }
        if other.wait_for_frame_completion_before_queueing {
            self.wait_for_frame_completion_before_queueing = true;
        }
        if other.enable_overlay_planes {
            self.enable_overlay_planes = true;
        }
        if other.disable_cursor_plane {
            self.disable_cursor_plane = true;
        }
        if other.disable_direct_scanout {
            self.disable_direct_scanout = true;
        }
        if other.restrict_primary_scanout_to_matching_format {
            self.restrict_primary_scanout_to_matching_format = true;
        }
        if other.render_drm_device.is_some() {
            self.render_drm_device = other.render_drm_device.clone();
        }
        if other.force_pipewire_invalid_modifier {
            self.force_pipewire_invalid_modifier = true;
        }
        if other.emulate_zero_presentation_time {
            self.emulate_zero_presentation_time = true;
        }
        if other.disable_resize_throttling {
            self.disable_resize_throttling = true;
        }
        if other.disable_transactions {
            self.disable_transactions = true;
        }
        if other.keep_laptop_panel_on_when_lid_is_closed {
            self.keep_laptop_panel_on_when_lid_is_closed = true;
        }
        if other.disable_monitor_names {
            self.disable_monitor_names = true;
        }
        if other.strict_new_window_focus_policy {
            self.strict_new_window_focus_policy = true;
        }
        if other.honor_xdg_activation_with_invalid_serial {
            self.honor_xdg_activation_with_invalid_serial = true;
        }
        if other.deactivate_unfocused_windows {
            self.deactivate_unfocused_windows = true;
        }
        if other.skip_cursor_only_updates_during_vrr {
            self.skip_cursor_only_updates_during_vrr = true;
        }
    }
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRender {
    Screencast,
    ScreenCapture,
}
