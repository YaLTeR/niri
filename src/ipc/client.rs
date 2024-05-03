use anyhow::Context;
use niri_ipc::{
    ActionRequest, Error, ErrorRequest, FocusedWindowRequest, LogicalOutput, Mode, NiriSocket,
    Output, OutputRequest, Request, VersionRequest,
};
use serde_json::json;

use crate::cli::Msg;
use crate::utils::version;

struct CompositorError {
    error: niri_ipc::Error,
    version: Option<String>,
}

trait MsgRequest: Request {
    fn json(response: Self::Response) -> serde_json::Value {
        json!(response)
    }

    fn show_to_human(response: Self::Response);

    fn check_version() -> Option<String> {
        if let Ok(Ok(version)) = VersionRequest.send() {
            Some(version)
        } else {
            None
        }
    }

    fn send(self) -> anyhow::Result<Result<Self::Response, CompositorError>> {
        let socket = NiriSocket::new().context("problem initializing the socket")?;
        let reply = socket.send_request(self).context("problem ")?;
        Ok(reply.map_err(|error| CompositorError {
            error,
            version: Self::check_version(),
        }))
    }
}

pub fn handle_msg(msg: Msg, json: bool) -> anyhow::Result<()> {
    match msg {
        Msg::RequestError { message } => run(json, ErrorRequest(message)),
        Msg::Version => run(json, VersionRequest),
        Msg::Outputs => run(json, OutputRequest),
        Msg::FocusedWindow => run(json, FocusedWindowRequest),
        Msg::Action { action } => run(json, ActionRequest(action)),
    }
}

fn run<R: MsgRequest>(json: bool, request: R) -> anyhow::Result<()> {
    let reply = request.send().context("a communication error occurred")?;

    // Piping `niri msg` into a command like `jq invalid` will cause jq to exit early
    // from the invalid expression. That also causes the pipe to close, and the piped process
    // receives a SIGPIPE. Normally, this would cause println! to panic, but because the error
    // ultimately doesn't originate in niri, and it's not a bug in niri, the resulting backtrace is
    // quite unhelpful to the user considering that the actual error (invalid jq expression) is
    // already shown on the terminal.
    //
    // To avoid this, we ignore any SIGPIPE we receive from here on out. This can potentially
    // interfere with IPC code, so we ensure that it is already finished by the time we reach this
    // point. Actual errors with the IPC code are not handled by us; they're bubbled up to
    // main() as Err(_). Those are separate from the pipe closing; and should be printed anyways.
    // But after this point, we only really print things, so it's safe to ignore SIGPIPE.
    // And since stdio panics are the *only* error path, we can be confident that there is actually
    // no error path from this point on.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    match reply {
        Ok(response) => {
            if json {
                println!("{}", R::json(response));
            } else {
                R::show_to_human(response);
            }
        }
        Err(CompositorError {
            error,
            version: server_version,
        }) => {
            match error {
                Error::ClientBadJson => {
                    eprintln!("Something went wrong in the CLI; the compositor says the JSON it sent was invalid")
                }
                Error::ClientBadProtocol => {
                    eprintln!("The compositor didn't understand the request sent by the CLI.")
                }
                Error::CompositorBadProtocol => {
                    eprintln!("The compositor returned a response that the CLI didn't understand.")
                }
                Error::InternalError => {
                    eprintln!("Something went wrong in the compositor. I don't know what.")
                }
                Error::Other(msg) => {
                    eprintln!("The compositor returned an error:");
                    eprintln!();
                    eprintln!("{msg}");
                }
            }

            if let Some(server_version) = server_version {
                let my_version = version();
                if my_version != server_version {
                    eprintln!();
                    eprintln!("Note: niri msg was invoked with a different version of niri than the running compositor.");
                    eprintln!("niri msg: {my_version}");
                    eprintln!("compositor: {server_version}");
                    eprintln!("Did you forget to restart niri after an update?");
                }
            } else {
                eprintln!();
                eprintln!("Note: unable to get the compositor's version.");
                eprintln!("Did you forget to restart niri after an update?");
            }
        }
    }

    Ok(())
}

impl MsgRequest for ErrorRequest {
    fn json(response: Self::Response) -> serde_json::Value {
        match response {}
    }

    fn show_to_human(response: Self::Response) {
        match response {}
    }
}

impl MsgRequest for VersionRequest {
    fn check_version() -> Option<String> {
        eprintln!("version");
        // If the version request fails, we can't exactly try again.
        None
    }
    fn json(response: Self::Response) -> serde_json::Value {
        json!({
            "cli": version(),
            "compositor": response,
        })
    }
    fn show_to_human(response: Self::Response) {
        let client_version = version();
        let server_version = response;
        println!("niri msg is {client_version}");
        println!("the compositor is {server_version}");
        if client_version != server_version {
            eprintln!();
            eprintln!("These are different");
            eprintln!("Did you forget to restart niri after an update?");
        }
        println!();
    }
}

impl MsgRequest for OutputRequest {
    fn show_to_human(response: Self::Response) {
        for (connector, output) in response {
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

            match current_mode.map(|idx| modes.get(idx)) {
                None => println!("  Disabled"),
                Some(None) => println!("  Current mode: (invalid index)"),
                Some(Some(&Mode {
                    width,
                    height,
                    refresh_rate,
                    is_preferred,
                })) => {
                    let refresh = refresh_rate as f64 / 1000.;
                    let preferred = if is_preferred { " (preferred)" } else { "" };
                    println!("  Current mode: {width}x{height} @ {refresh:.3} Hz{preferred}");
                }
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
                    niri_ipc::Transform::Normal => "normal",
                    niri_ipc::Transform::_90 => "90° counter-clockwise",
                    niri_ipc::Transform::_180 => "180°",
                    niri_ipc::Transform::_270 => "270° counter-clockwise",
                    niri_ipc::Transform::Flipped => "flipped horizontally",
                    niri_ipc::Transform::Flipped90 => "90° counter-clockwise, flipped horizontally",
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
}

impl MsgRequest for FocusedWindowRequest {
    fn show_to_human(response: Self::Response) {
        if let Some(window) = response {
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
}

impl MsgRequest for ActionRequest {
    fn show_to_human(response: Self::Response) {
        response
    }
}
