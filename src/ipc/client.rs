use anyhow::{anyhow, bail, Context};
use niri_ipc::{
    Event, LogicalOutput, Mode, Output, OutputConfigChanged, Request, Response, Socket, Transform,
};
use serde_json::json;

use crate::cli::Msg;
use crate::utils::version;

pub fn handle_msg(msg: Msg, json: bool) -> anyhow::Result<()> {
    let request = match &msg {
        Msg::Version => Request::Version,
        Msg::Outputs => Request::Outputs,
        Msg::FocusedWindow => Request::FocusedWindow,
        Msg::FocusedOutput => Request::FocusedOutput,
        Msg::Action { action } => Request::Action(action.clone()),
        Msg::Output { output, action } => Request::Output {
            output: output.clone(),
            action: action.clone(),
        },
        Msg::Workspaces => Request::Workspaces,
        Msg::EventStream => Request::EventStream,
        Msg::RequestError => Request::ReturnError,
    };

    let socket = Socket::connect().context("error connecting to the niri socket")?;

    let (reply, mut read_event) = socket
        .send(request)
        .context("error communicating with niri")?;

    let compositor_version = match reply {
        Err(_) if !matches!(msg, Msg::Version) => {
            // If we got an error, it might be that the CLI is a different version from the running
            // niri instance. Request the running instance version to compare and print a message.
            Socket::connect()
                .and_then(|socket| socket.send(Request::Version))
                .ok()
                .map(|(reply, _read_event)| reply)
        }
        _ => None,
    };

    // Default SIGPIPE so that our prints don't panic on stdout closing.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let response = reply.map_err(|err_msg| {
        // Check for CLI-server version mismatch to add helpful context.
        match compositor_version {
            Some(Ok(Response::Version(compositor_version))) => {
                let cli_version = version();
                if cli_version != compositor_version {
                    eprintln!("Running niri compositor has a different version from the niri CLI:");
                    eprintln!("Compositor version: {compositor_version}");
                    eprintln!("CLI version:        {cli_version}");
                    eprintln!("Did you forget to restart niri after an update?");
                    eprintln!();
                }
            }
            Some(_) => {
                eprintln!("Unable to get the running niri compositor version.");
                eprintln!("Did you forget to restart niri after an update?");
                eprintln!();
            }
            None => {
                // Communication error, or the original request was already a version request.
                // Don't add irrelevant context.
            }
        }

        anyhow!(err_msg).context("niri returned an error")
    })?;

    match msg {
        Msg::RequestError => {
            bail!("unexpected response: expected an error, got {response:?}");
        }
        Msg::Version => {
            let Response::Version(compositor_version) = response else {
                bail!("unexpected response: expected Version, got {response:?}");
            };

            let cli_version = version();

            if json {
                println!(
                    "{}",
                    json!({
                        "compositor": compositor_version,
                        "cli": cli_version,
                    })
                );
                return Ok(());
            }

            if cli_version != compositor_version {
                eprintln!("Running niri compositor has a different version from the niri CLI.");
                eprintln!("Did you forget to restart niri after an update?");
                eprintln!();
            }

            println!("Compositor version: {compositor_version}");
            println!("CLI version:        {cli_version}");
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
                print_output(connector, output)?;
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
        Msg::FocusedOutput => {
            let Response::FocusedOutput(output) = response else {
                bail!("unexpected response: expected FocusedOutput, got {response:?}");
            };

            if json {
                let output = serde_json::to_string(&output).context("error formatting response")?;
                println!("{output}");
                return Ok(());
            }

            if let Some(output) = output {
                print_output(output.name.clone(), output)?;
            } else {
                println!("No output is focused.");
            }
        }
        Msg::Action { .. } => {
            let Response::Handled = response else {
                bail!("unexpected response: expected Handled, got {response:?}");
            };
        }
        Msg::Output { output, .. } => {
            let Response::OutputConfigChanged(response) = response else {
                bail!("unexpected response: expected OutputConfigChanged, got {response:?}");
            };

            if json {
                let response =
                    serde_json::to_string(&response).context("error formatting response")?;
                println!("{response}");
                return Ok(());
            }

            if response == OutputConfigChanged::OutputWasMissing {
                println!("Output \"{output}\" is not connected.");
                println!("The change will apply when it is connected.");
            }
        }
        Msg::Workspaces => {
            let Response::Workspaces(mut response) = response else {
                bail!("unexpected response: expected Workspaces, got {response:?}");
            };

            if json {
                let response =
                    serde_json::to_string(&response).context("error formatting response")?;
                println!("{response}");
                return Ok(());
            }

            if response.is_empty() {
                println!("No workspaces.");
                return Ok(());
            }

            response.sort_by_key(|ws| ws.idx);
            response.sort_by(|a, b| a.output.cmp(&b.output));

            let mut current_output = if let Some(output) = response[0].output.as_deref() {
                println!("Output \"{output}\":");
                Some(output)
            } else {
                println!("No output:");
                None
            };

            for ws in &response {
                if ws.output.as_deref() != current_output {
                    let output = ws.output.as_deref().context(
                        "invalid response: workspace with no output \
                         following a workspace with an output",
                    )?;
                    current_output = Some(output);
                    println!("\nOutput \"{output}\":");
                }

                let is_active = if ws.is_active { " * " } else { "   " };
                let idx = ws.idx;
                let name = if let Some(name) = ws.name.as_deref() {
                    format!(" \"{name}\"")
                } else {
                    String::new()
                };
                println!("{is_active}{idx}{name}");
            }
        }
        Msg::EventStream => {
            let Response::Handled = response else {
                bail!("unexpected response: expected Handled, got {response:?}");
            };

            println!("Started reading events.");

            loop {
                let event = read_event().context("error reading event from niri")?;
                match event {
                    Event::WorkspaceCreated { workspace } => {
                        println!("Workspace created: {workspace:?}");
                    }
                    Event::WorkspaceRemoved { id } => {
                        println!("Workspace removed: {id}");
                    }
                    Event::WorkspaceSwitched { output, id } => {
                        println!("Workspace switched on output \"{output}\": {id}");
                    }
                    Event::WorkspaceMoved { id, output, idx } => {
                        println!("Workspace moved: {id} to output \"{output}\", index {idx}");
                    }
                    Event::WindowFocused { window } => {
                        println!("Window focused: {window:?}");
                    }
                    Event::KeyboardLayoutChanged { name } => {
                        println!("Keyboard layout changed: \"{name}\"");
                    }
                }
            }
        }
    }

    Ok(())
}

fn print_output(connector: String, output: Output) -> anyhow::Result<()> {
    let Output {
        name,
        make,
        model,
        physical_size,
        modes,
        current_mode,
        vrr_supported,
        vrr_enabled,
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

    if vrr_supported {
        let enabled = if vrr_enabled { "enabled" } else { "disabled" };
        println!("  Variable refresh rate: supported, {enabled}");
    } else {
        println!("  Variable refresh rate: not supported");
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
            Transform::Normal => "normal",
            Transform::_90 => "90° counter-clockwise",
            Transform::_180 => "180°",
            Transform::_270 => "270° counter-clockwise",
            Transform::Flipped => "flipped horizontally",
            Transform::Flipped90 => "90° counter-clockwise, flipped horizontally",
            Transform::Flipped180 => "flipped vertically",
            Transform::Flipped270 => "270° counter-clockwise, flipped horizontally",
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
    Ok(())
}
