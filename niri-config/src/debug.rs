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
    pub keep_max_bpc_unchanged: bool,
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

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRender {
    Screencast,
    ScreenCapture,
}
