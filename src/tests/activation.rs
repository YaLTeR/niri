use std::time::Duration;

use niri_config::Config;
use smithay::reexports::wayland_server::Resource as _;
use smithay::wayland::xdg_activation::XdgActivationHandler as _;
use wayland_client::Proxy as _;

use super::client::ClientId;
use super::Fixture;

fn map_window(
    f: &mut Fixture,
    client: ClientId,
    app_id: &str,
) -> wayland_client::protocol::wl_surface::WlSurface {
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.xdg_toplevel.set_app_id(app_id.to_owned());
    window.commit();
    f.roundtrip(client);
    let window = f.client(client).window(&surface);
    window.attach_new_buffer();
    window.set_size(400, 300);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    surface
}

fn check_activation(
    app_id: &str,
    action: Option<&str>,
    urgency_only: bool,
    expired: bool,
    should_focus: bool,
    should_be_urgent: bool,
) {
    let mut config = Config::parse_mem(
        r#"
        debug { honor-xdg-activation-with-invalid-serial; }
        "#,
    )
    .unwrap();
    if let Some(action) = action {
        let rule_config = Config::parse_mem(&format!(
            r#"window-rule {{ match app-id="(?i)^google-chrome$"; on-xdg-activate "{action}"; }}"#,
        ))
        .unwrap();
        config.window_rules = rule_config.window_rules;
    }
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1920, 1080));
    let client = f.add_client();
    f.niri_focus_output(1);
    let target = map_window(&mut f, client, app_id);
    let server_surface = f
        .niri()
        .layout
        .windows()
        .find(|(_, mapped)| {
            mapped.toplevel().wl_surface().id().protocol_id() == target.id().protocol_id()
        })
        .unwrap()
        .1
        .toplevel()
        .wl_surface()
        .clone();

    f.niri_focus_output(2);
    let editor = map_window(&mut f, client, "codex-desktop");
    f.niri_complete_animations();
    assert_eq!(f.client(client).keyboard_focus(), Some(&editor));
    let user_output = f.niri_output(2);
    assert_eq!(f.niri().layout.active_output(), Some(&user_output));

    // Exercise the real handler after token acceptance, the boundary changed by
    // this backport. The global compatibility flag remains enabled throughout.
    let (token, mut data) = {
        let (token, data) = f.niri().activation_state.create_external_token(None);
        (token.clone(), data.clone())
    };
    if urgency_only {
        assert!(f.niri_state().token_created(token.clone(), data.clone()));
    }
    if expired {
        data.timestamp -= Duration::from_secs(20);
    }
    f.niri_state()
        .request_activation(token.clone(), data, server_surface.clone());
    f.double_roundtrip(client);
    f.niri_complete_animations();

    let expected_output = if should_focus {
        f.niri_output(1)
    } else {
        user_output
    };
    assert_eq!(
        f.niri().layout.active_output(),
        Some(&expected_output),
        "{app_id}"
    );
    let expected_focus = if should_focus { &target } else { &editor };
    assert_eq!(
        f.client(client).keyboard_focus(),
        Some(expected_focus),
        "{app_id}"
    );
    let urgent = f
        .niri()
        .layout
        .windows()
        .find(|(_, mapped)| mapped.toplevel().wl_surface() == &server_surface)
        .unwrap()
        .1
        .is_urgent();
    assert_eq!(urgent, should_be_urgent, "{app_id}");
    assert!(f.niri().activation_state.data_for_token(&token).is_none());

    // Explicit user/compositor focus must still work even when the application
    // is not allowed to focus itself through xdg-activation.
    if !should_focus {
        let window = f
            .niri()
            .layout
            .windows()
            .find(|(_, mapped)| mapped.toplevel().wl_surface() == &server_surface)
            .unwrap()
            .1
            .window
            .clone();
        f.niri().layout.activate_window(&window);
        f.double_roundtrip(client);
        assert_eq!(f.client(client).keyboard_focus(), Some(&target));
    }
}

#[test]
fn chrome_activation_keeps_user_focus_with_compatibility_enabled() {
    for app_id in ["Google-chrome", "google-chrome"] {
        check_activation(app_id, Some("set-urgent"), false, false, false, true);
    }
}

#[test]
fn chrome_rule_preserves_other_apps_and_default_activation() {
    for app_id in ["firefox", "org.telegram.desktop", "Google-chrome-other"] {
        check_activation(app_id, Some("set-urgent"), false, false, true, false);
    }
    check_activation("Google-chrome", None, false, false, true, false);
    check_activation("firefox", Some("set-urgent"), true, false, false, true);
}

#[test]
fn activation_actions_and_expiry_keep_upstream_semantics() {
    check_activation("Google-chrome", Some("ignore"), false, false, false, false);
    check_activation("Google-chrome", Some("focus"), true, false, true, false);
    check_activation(
        "Google-chrome",
        Some("set-urgent"),
        false,
        true,
        false,
        false,
    );
}
