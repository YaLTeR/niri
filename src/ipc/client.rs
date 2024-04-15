use anyhow::{bail, Context};
use niri_ipc::{LogicalOutput, Mode, NiriSocket, Output, Request, Response};
use serde_json::json;

use crate::cli::Msg;
use crate::utils::version;

pub fn handle_msg(msg: Msg, json: bool) -> anyhow::Result<()> {
    let mut client = NiriSocket::new()
        .context("a communication error occured while trying to initialize the socket")?;

    // Default SIGPIPE so that our prints don't panic on stdout closing.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let request = match &msg {
        Msg::Error => Request::ReturnError,
        Msg::Version => Request::Version,
        Msg::Outputs => Request::Outputs,
        Msg::FocusedWindow => Request::FocusedWindow,
        Msg::Action { action } => Request::Action(action.clone()),
    };

    let reply = client
        .send(request)
        .context("a communication error occurred while sending request to niri")?;

    let response = match reply {
        Ok(r) => r,
        Err(err_msg) => {
            eprintln!("The compositor returned an error:");
            eprintln!();
            eprintln!("{err_msg}");

            if matches!(msg, Msg::Version) {
                eprintln!();
                eprintln!("Note: unable to get the compositor's version.");
                eprintln!("Did you forget to restart niri after an update?");
            } else {
                // We're making a new client here just for some vague notion of
                // backwards compatibility.
                // It is in general not necessary to do so.
                match NiriSocket::new().and_then(|mut client| client.send(Request::Version)) {
                    Ok(Ok(Response::Version(server_version))) => {
                        let my_version = version();
                        if my_version != server_version {
                            eprintln!();
                            eprintln!("Note: niri msg was invoked with a different version of niri than the running compositor.");
                            eprintln!("niri msg: {my_version}");
                            eprintln!("compositor: {server_version}");
                            eprintln!("Did you forget to restart niri after an update?");
                        }
                    }
                    Ok(Ok(_)) => {
                        // nonsensical response, do not add confusing context
                    }
                    Ok(Err(_)) => {
                        eprintln!();
                        eprintln!("Note: unable to get the compositor's version.");
                        eprintln!("Did you forget to restart niri after an update?");
                    }
                    Err(_) => {
                        // communication error, do not add irrelevant context
                    }
                }
            }

            return Ok(());
        }
    };

    match msg {
        Msg::Error => {
            bail!("unexpected response: expected an error, got {response:?}");
        }
        Msg::Version => {
            let Response::Version(server_version) = response else {
                bail!("unexpected response: expected Version, got {response:?}");
            };

            if json {
                println!(
                    "{}",
                    json!({
                        "cli": version(),
                        "compositor": server_version,
                    })
                );
                return Ok(());
            }

            let client_version = version();

            println!("niri msg is {client_version}");
            println!("the compositor is {server_version}");
            if client_version != server_version {
                eprintln!();
                eprintln!("These are different");
                eprintln!("Did you forget to restart niri after an update?");
            }
            println!();
        }
        Msg::Outputs => {
            let Response::Outputs(outputs) = response else {
                bail!("unexpected response: expected Outputs, got {response:?}");
            };

            if json {
                let output =
                    serde_json::to_string(&outputs).context("error formatting response")?;
                println!("{output}");
                return Ok(());
            }

            let mut outputs = outputs.into_iter().collect::<Vec<_>>();
            outputs.sort_unstable_by(|a, b| a.0.cmp(&b.0));

            for (connector, output) in outputs.into_iter() {
                let Output {
                    name,
                    make,
                    model,
                    physical_size,
                    modes,
                    current_mode,
                    logical,
                } = output;

                println!(r#"Output "{connector}" ({make} - {model} - {name})"#);

                if let Some(current) = current_mode {
                    let mode = *modes
                        .get(current)
                        .context("invalid response: current mode does not exist")?;
                    let Mode {
                        width,
                        height,
                        refresh_rate,
                        is_preferred,
                    } = mode;
                    let refresh = refresh_rate as f64 / 1000.;
                    let preferred = if is_preferred { " (preferred)" } else { "" };
                    println!("  Current mode: {width}x{height} @ {refresh:.3} Hz{preferred}");
                } else {
                    println!("  Disabled");
                }

                if let Some((width, height)) = physical_size {
                    println!("  Physical size: {width}x{height} mm");
                } else {
                    println!("  Physical size: unknown");
                }

                if let Some(logical) = logical {
                    let LogicalOutput {
                        x,
                        y,
                        width,
                        height,
                        scale,
                        transform,
                    } = logical;
                    println!("  Logical position: {x}, {y}");
                    println!("  Logical size: {width}x{height}");
                    println!("  Scale: {scale}");

                    let transform = match transform {
                        niri_ipc::Transform::Normal => "normal",
                        niri_ipc::Transform::_90 => "90° counter-clockwise",
                        niri_ipc::Transform::_180 => "180°",
                        niri_ipc::Transform::_270 => "270° counter-clockwise",
                        niri_ipc::Transform::Flipped => "flipped horizontally",
                        niri_ipc::Transform::Flipped90 => {
                            "90° counter-clockwise, flipped horizontally"
                        }
                        niri_ipc::Transform::Flipped180 => "flipped vertically",
                        niri_ipc::Transform::Flipped270 => {
                            "270° counter-clockwise, flipped horizontally"
                        }
                    };
                    println!("  Transform: {transform}");
                }

                println!("  Available modes:");
                for (idx, mode) in modes.into_iter().enumerate() {
                    let Mode {
                        width,
                        height,
                        refresh_rate,
                        is_preferred,
                    } = mode;
                    let refresh = refresh_rate as f64 / 1000.;

                    let is_current = Some(idx) == current_mode;
                    let qualifier = match (is_current, is_preferred) {
                        (true, true) => " (current, preferred)",
                        (true, false) => " (current)",
                        (false, true) => " (preferred)",
                        (false, false) => "",
                    };

                    println!("    {width}x{height}@{refresh:.3}{qualifier}");
                }
                println!();
            }
        }
        Msg::FocusedWindow => {
            let Response::FocusedWindow(window) = response else {
                bail!("unexpected response: expected FocusedWindow, got {response:?}");
            };

            if json {
                let window = serde_json::to_string(&window).context("error formatting response")?;
                println!("{window}");
                return Ok(());
            }

            if let Some(window) = window {
                println!("Focused window:");

                if let Some(title) = window.title {
                    println!("  Title: \"{title}\"");
                } else {
                    println!("  Title: (unset)");
                }

                if let Some(app_id) = window.app_id {
                    println!("  App ID: \"{app_id}\"");
                } else {
                    println!("  App ID: (unset)");
                }
            } else {
                println!("No window is focused.");
            }
        }
        Msg::Action { .. } => {
            let Response::Handled = response else {
                bail!("unexpected response: expected Handled, got {response:?}");
            };
        }
    }

    Ok(())
}
