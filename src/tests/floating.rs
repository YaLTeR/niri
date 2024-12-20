use client::ClientId;
use insta::assert_snapshot;
use niri_ipc::SizeChange;
use wayland_client::protocol::wl_surface::WlSurface;

use super::*;

// Sets up a fixture with two outputs and 100×100 window.
fn set_up() -> (Fixture, ClientId, WlSurface) {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1280, 720));

    let id = f.add_client();
    let window = f.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(id);

    let window = f.client(id).window(&surface);
    window.attach_new_buffer();
    window.set_size(100, 100);
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    (f, id, surface)
}

#[test]
fn unfocus_preserves_current_size() {
    let (mut f, id, surface) = set_up();

    f.niri().layout.toggle_window_floating(None);
    f.roundtrip(id);

    // Change window size while it's floating.
    let window = f.client(id).window(&surface);
    window.set_size(200, 200);
    window.ack_last_and_commit();
    f.double_roundtrip(id);

    let _ = f.client(id).window(&surface).recent_configures();

    // Focus a different output which should drop the Activated state.
    f.niri_focus_output(2);

    f.double_roundtrip(id);

    // This should request 200 × 200 because that's the current window size.
    let window = f.client(id).window(&surface);
    assert_snapshot!(
        window.format_recent_configures(),
        @"size: 200 × 200, bounds: 1920 × 1080, states: []"
    );

    // Change window size again.
    let window = f.client(id).window(&surface);
    window.set_size(300, 300);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // Focus the first output which should add back the Activated state.
    f.niri_focus_output(1);

    f.double_roundtrip(id);

    // This should request 300 × 300 because that's the current window size.
    let window = f.client(id).window(&surface);
    assert_snapshot!(
        window.format_recent_configures(),
        @"size: 300 × 300, bounds: 1920 × 1080, states: [Activated]"
    );
}

#[test]
fn resize_to_different_size() {
    let (mut f, id, surface) = set_up();
    let _ = f.client(id).window(&surface).recent_configures();

    f.niri().layout.toggle_window_floating(None);
    f.niri().layout.set_column_width(SizeChange::SetFixed(500));
    f.double_roundtrip(id);

    // This should request the new size, 500 × 100.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 100, bounds: 1920 × 1080, states: [Activated]"
    );

    // Focus a different output which should drop the Activated state.
    f.niri_focus_output(2);
    f.double_roundtrip(id);
    // This should request the new size since the window hasn't committed yet.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 100, bounds: 1920 × 1080, states: []"
    );

    // Ack but don't commit yet.
    let window = f.client(id).window(&surface);
    window.ack_last();
    f.roundtrip(id);
    // Add the activated state.
    f.niri_focus_output(1);
    f.double_roundtrip(id);
    // This should request the new size since the window hasn't committed yet.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 100, bounds: 1920 × 1080, states: [Activated]"
    );

    // Commit but with some different size.
    let window = f.client(id).window(&surface);
    window.set_size(200, 200);
    window.commit();
    f.double_roundtrip(id);
    // This shouldn't request anything.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @""
    );

    // Drop the Activated state.
    f.niri_focus_output(2);
    f.double_roundtrip(id);
    // This should request the current window size rather than keep requesting 500 × 100.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 200 × 200, bounds: 1920 × 1080, states: []"
    );
}

#[test]
fn set_window_width_uses_current_height() {
    let (mut f, id, surface) = set_up();

    f.niri().layout.toggle_window_floating(None);
    f.double_roundtrip(id);
    let _ = f.client(id).window(&surface).recent_configures();

    // Resize to something different on both axes.
    let window = f.client(id).window(&surface);
    window.set_size(200, 200);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // Request a width change.
    f.niri().layout.set_column_width(SizeChange::SetFixed(500));

    f.double_roundtrip(id);

    // This should use the current window height (200), rather than the initial window height (100).
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 200, bounds: 1920 × 1080, states: [Activated]"
    );
}

#[test]
fn set_window_height_uses_current_width() {
    let (mut f, id, surface) = set_up();

    f.niri().layout.toggle_window_floating(None);
    f.double_roundtrip(id);
    let _ = f.client(id).window(&surface).recent_configures();

    // Resize to something different on both axes.
    let window = f.client(id).window(&surface);
    window.set_size(200, 200);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // Request a width change.
    f.niri()
        .layout
        .set_window_height(None, SizeChange::SetFixed(500));

    f.double_roundtrip(id);

    // This should use the current window width (200), rather than the initial window width (100).
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 200 × 500, bounds: 1920 × 1080, states: [Activated]"
    );
}

#[test]
fn resize_to_same_size() {
    let (mut f, id, surface) = set_up();

    f.niri().layout.toggle_window_floating(None);
    f.double_roundtrip(id);
    let _ = f.client(id).window(&surface).recent_configures();

    // Resize to something different.
    let window = f.client(id).window(&surface);
    window.set_size(200, 200);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // Request a size change to the same size.
    f.niri().layout.set_column_width(SizeChange::SetFixed(200));

    f.double_roundtrip(id);

    // This needn't request anything because we're already that size; the size in the current
    // server state matches the requested size.
    //
    // FIXME: However, currently it will request the size anyway because the code checks the
    // current server state, and the last size niri requested of the window was 100×100 (even if
    // the window already acked and committed in response).
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 200 × 200, bounds: 1920 × 1080, states: [Activated]"
    );
}

#[test]
fn resize_to_different_then_same() {
    let (mut f, id, surface) = set_up();

    f.niri().layout.toggle_window_floating(None);
    f.double_roundtrip(id);
    let _ = f.client(id).window(&surface).recent_configures();

    // Commit in response to any configure from the floating change.
    let window = f.client(id).window(&surface);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // Request a size change to a different size.
    f.niri().layout.set_column_width(SizeChange::SetFixed(500));

    f.double_roundtrip(id);

    // This should request the new size.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 100, bounds: 1920 × 1080, states: [Activated]"
    );

    // Before the window has a chance to respond, request a size change to the same, new size.
    f.niri().layout.set_column_width(SizeChange::SetFixed(500));

    // And also drop the Activated state to have some pending change.
    f.niri_focus_output(2);

    f.double_roundtrip(id);

    // This should keep requesting the new size (500 × 100) since the window has not responded to
    // it yet.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 500 × 100, bounds: 1920 × 1080, states: []"
    );

    // Commit in response to the size change request.
    let window = f.client(id).window(&surface);
    window.set_size(300, 300);
    window.ack_last_and_commit();
    f.roundtrip(id);

    // And also add the Activated state to have some pending change.
    f.niri_focus_output(1);

    f.double_roundtrip(id);

    // This should request the current window size (300 × 300) since the window has committed in
    // response to the size change.
    assert_snapshot!(
        f.client(id).window(&surface).format_recent_configures(),
        @"size: 300 × 300, bounds: 1920 × 1080, states: [Activated]"
    );
}
