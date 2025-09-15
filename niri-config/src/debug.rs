use std::path::PathBuf;

use niri_macros::Mergeable;

use crate::maybe_set::BoolFlag;

#[derive(knuffel::Decode, Debug, Default, PartialEq, Clone, Mergeable)]
pub struct Debug {
    #[knuffel(child, unwrap(argument))]
    pub preview_render: Option<PreviewRender>,
    #[knuffel(child, default)]
    pub dbus_interfaces_in_non_session_instances: BoolFlag,
    #[knuffel(child, default)]
    pub wait_for_frame_completion_before_queueing: BoolFlag,
    #[knuffel(child, default)]
    pub enable_overlay_planes: BoolFlag,
    #[knuffel(child, default)]
    pub disable_cursor_plane: BoolFlag,
    #[knuffel(child, default)]
    pub disable_direct_scanout: BoolFlag,
    #[knuffel(child, default)]
    pub keep_max_bpc_unchanged: BoolFlag,
    #[knuffel(child, default)]
    pub restrict_primary_scanout_to_matching_format: BoolFlag,
    #[knuffel(child, unwrap(argument))]
    pub render_drm_device: Option<PathBuf>,
    #[knuffel(child, default)]
    pub force_pipewire_invalid_modifier: BoolFlag,
    #[knuffel(child, default)]
    pub emulate_zero_presentation_time: BoolFlag,
    #[knuffel(child, default)]
    pub disable_resize_throttling: BoolFlag,
    #[knuffel(child, default)]
    pub disable_transactions: BoolFlag,
    #[knuffel(child, default)]
    pub keep_laptop_panel_on_when_lid_is_closed: BoolFlag,
    #[knuffel(child, default)]
    pub disable_monitor_names: BoolFlag,
    #[knuffel(child, default)]
    pub strict_new_window_focus_policy: BoolFlag,
    #[knuffel(child, default)]
    pub honor_xdg_activation_with_invalid_serial: BoolFlag,
    #[knuffel(child, default)]
    pub deactivate_unfocused_windows: BoolFlag,
    #[knuffel(child, default)]
    pub skip_cursor_only_updates_during_vrr: BoolFlag,
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq, Eq, Mergeable)]
pub enum PreviewRender {
    Screencast,
    ScreenCapture,
}

#[cfg(test)]
mod tests {
    use niri_macros::Mergeable;

    use super::*;
    use crate::{MaybeSet, Mergeable};

    #[test]
    fn test_debug_regular_bool_merging() {
        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct SingleFlag {
            pub test_flag: bool,
        }

        let mut base = SingleFlag::default();
        assert!(!base.test_flag);

        let overlay = SingleFlag { test_flag: true };
        base.merge_with(&overlay);
        assert!(base.test_flag);

        let overlay2 = SingleFlag { test_flag: false };
        base.merge_with(&overlay2);
        assert!(!base.test_flag);
    }

    #[test]
    fn test_debug_mutex_behavior_with_multiple_bool_flags() {
        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct MultiFlags {
            pub flag_a: BoolFlag,
            pub flag_b: BoolFlag,
            pub flag_c: BoolFlag,
        }

        let mut base = MultiFlags::default();
        assert!(!*base.flag_a);
        assert!(!*base.flag_b);
        assert!(!*base.flag_c);

        let overlay = MultiFlags {
            flag_a: BoolFlag(MaybeSet::new(true)),
            ..Default::default()
        };

        base.merge_with(&overlay);
        assert!(base.flag_a.0.is_set());
        assert!(*base.flag_a);
        assert!(!*base.flag_b);
        assert!(!*base.flag_c);

        let overlay2 = MultiFlags {
            flag_b: BoolFlag(MaybeSet::new(true)),
            ..Default::default()
        };

        base.merge_with(&overlay2);
        assert!(*base.flag_a);
        assert!(*base.flag_b);
        assert!(!*base.flag_c);
    }

    #[test]
    fn test_debug_mutex_with_regular_fields() {
        let mut base = Debug::default();
        base.dbus_interfaces_in_non_session_instances = BoolFlag(MaybeSet::new(true));

        let overlay = Debug {
            wait_for_frame_completion_before_queueing: BoolFlag(MaybeSet::new(true)),
            render_drm_device: Some("/dev/dri/renderD128".into()),
            ..Default::default()
        };

        base.merge_with(&overlay);

        assert!(*base.dbus_interfaces_in_non_session_instances);
        assert!(*base.wait_for_frame_completion_before_queueing);
        assert_eq!(
            base.render_drm_device.as_deref(),
            Some("/dev/dri/renderD128".as_ref())
        );
    }

    #[test]
    fn test_mutex_behavior_with_regular_bools() {
        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct MultiBools {
            pub option_a: bool,
            pub option_b: bool,
            pub option_c: bool,
        }

        let mut base = MultiBools::default();
        assert!(!base.option_a);
        assert!(!base.option_b);
        assert!(!base.option_c);

        let overlay = MultiBools {
            option_a: true,
            ..Default::default()
        };

        base.merge_with(&overlay);
        assert!(base.option_a);
        assert!(!base.option_b);
        assert!(!base.option_c);

        let overlay2 = MultiBools {
            option_b: true,
            ..Default::default()
        };

        base.merge_with(&overlay2);
        assert!(!base.option_a);
        assert!(base.option_b);
        assert!(!base.option_c);
    }
}
