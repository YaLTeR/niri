use niri_ipc::PositionChange;
use smithay::reexports::wayland_server::Resource as _;
use smithay::utils::Point;
use wayland_client::Proxy as _;

use super::*;
use crate::ipc::server::window_geometry;

#[test]
fn live_window_geometry_is_targeted_and_independent_of_null_ipc_tile_positions() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(client);
    let window = f.client(client).window(&surface);
    window.attach_new_buffer();
    window.set_size(400, 300);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    f.niri_complete_animations();
    let mut target = None;
    f.niri().layout.with_windows(|mapped, _, _, layout| {
        assert!(layout.tile_pos_in_workspace_view.is_none());
        target = Some(mapped.id().get());
    });
    let target = target.unwrap();
    let geometry = window_geometry(f.niri_state(), target).unwrap();
    assert_eq!((geometry.width, geometry.height), (400, 300));
    assert!(geometry.x >= 0.0 && geometry.y >= 0.0);
    assert!(geometry.x + 400.0 <= 1920.0 && geometry.y + 300.0 <= 1080.0);
    assert_eq!(geometry.outputs, vec![(0, 0, 1920, 1080)]);
    assert!(window_geometry(f.niri_state(), target + 100).is_err());
    f.niri().layout.toggle_window_floating(None);
    f.double_roundtrip(client);
    f.niri_complete_animations();
    f.niri().layout.move_floating_window(
        None,
        PositionChange::SetFixed(600.0),
        PositionChange::SetFixed(400.0),
        false,
    );
    f.niri_complete_animations();
    let moved = window_geometry(f.niri_state(), target).unwrap();
    assert!(moved.x > geometry.x || moved.y > geometry.y);
    let hit = f
        .niri()
        .contents_under(Point::from((moved.x + 20.0, moved.y + 20.0)))
        .surface
        .unwrap()
        .0;
    assert_eq!(hit.id().protocol_id(), surface.id().protocol_id());
    // Layout activity alone must not authorize input through compositor overlays.
    let saved_focus = f.niri().keyboard_focus.clone();
    for focus in [
        crate::niri::KeyboardFocus::Mru,
        crate::niri::KeyboardFocus::ExitConfirmDialog,
        crate::niri::KeyboardFocus::LayerShell {
            surface: hit.clone(),
        },
    ] {
        f.niri().keyboard_focus = focus;
        assert!(window_geometry(f.niri_state(), target).is_err());
    }
    f.niri().keyboard_focus = saved_focus;
    assert!(window_geometry(f.niri_state(), target).is_ok());
    f.add_output(2, (1280, 720));
    f.niri_focus_output(2);
    f.double_roundtrip(client);
    assert!(window_geometry(f.niri_state(), target).is_err());
    f.niri_focus_output(1);
    f.double_roundtrip(client);
    f.niri().layout.open_overview();
    assert!(window_geometry(f.niri_state(), target).is_err());
    f.niri().layout.close_overview();
    f.niri_complete_animations();
    use smithay::reexports::wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer;
    use smithay::reexports::wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity};
    let layer = f
        .client(client)
        .create_layer(None, Layer::Overlay, "geometry-test");
    let layer_surface = layer.surface.clone();
    layer.set_configure_props(super::client::LayerConfigureProps {
        anchor: Some(Anchor::Top | Anchor::Left),
        size: Some((100, 100)),
        kb_interactivity: Some(KeyboardInteractivity::Exclusive),
        ..Default::default()
    });
    layer.commit();
    f.roundtrip(client);
    let layer = f.client(client).layer(&layer_surface);
    layer.attach_new_buffer();
    layer.set_size(100, 100);
    layer.ack_last_and_commit();
    f.double_roundtrip(client);
    assert_eq!(f.client(client).keyboard_focus(), Some(&layer_surface));
    assert!(window_geometry(f.niri_state(), target).is_err());
}
