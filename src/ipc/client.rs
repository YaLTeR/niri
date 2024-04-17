use anyhow::Context;
use niri_ipc::{
    ActionRequest, ErrorRequest, FocusedWindowRequest, LogicalOutput, Mode, NiriSocket, Output,
    OutputRequest, Request, VersionRequest,
};
use serde_json::json;

use crate::cli::Msg;
use crate::utils::version;

struct CompositorError {
    message: String,
    version: Option<String>,
}

type MsgResult<T> = Result<T, CompositorError>;

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

    fn send(self) -> anyhow::Result<MsgResult<Self::Response>> {
        let socket = NiriSocket::new().context("trying to initialize the socket")?;
        let reply = socket
            .send_request(self)
            .context("while sending request to niri")?;
        Ok(reply.map_err(|message| CompositorError {
            message,
            version: Self::check_version(),
        }))
    }
}

pub fn handle_msg(msg: Msg, json: bool) -> anyhow::Result<()> {
    match &msg {
        Msg::Error => run(json, ErrorRequest),
        Msg::Version => run(json, VersionRequest),
        Msg::Outputs => run(json, OutputRequest),
        Msg::FocusedWindow => run(json, FocusedWindowRequest),
        Msg::Action { action } => run(json, ActionRequest(action.clone())),
    }
}

fn run<R: MsgRequest>(json: bool, request: R) -> anyhow::Result<()> {
    let reply = request.send().context("a communication error occurred")?;

    // Default SIGPIPE so that our prints don't panic on stdout closing.
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
            message,
            version: server_version,
        }) => {
            eprintln!("The compositor returned an error:");
            eprintln!();
            eprintln!("{message}");

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

// fn _a() {

//     let reply = client
//         .send(request)
//         .context("a communication error occurred while sending request to niri")?;

//     let response = match reply {
//         Ok(r) => r,
//         Err(err_msg) => {
//             eprintln!("The compositor returned an error:");
//             eprintln!();
//             eprintln!("{err_msg}");

//             if matches!(msg, Msg::Version) {
//                 eprintln!();
//                 eprintln!("Note: unable to get the compositor's version.");
//                 eprintln!("Did you forget to restart niri after an update?");
//             } else {
//                 match NiriSocket::new().and_then(|client| client.send(RequestEnum::Version)) {
//                     Ok(Ok(Response::Version(server_version))) => {
//                         let my_version = version();
//                         if my_version != server_version {
//                             eprintln!();
//                             eprintln!("Note: niri msg was invoked with a different version of
// niri than the running compositor.");                             eprintln!("niri msg:
// {my_version}");                             eprintln!("compositor: {server_version}");
//                             eprintln!("Did you forget to restart niri after an update?");
//                         }
//                     }
//                     Ok(Ok(_)) => {
//                         // nonsensical response, do not add confusing context
//                     }
//                     Ok(Err(_)) => {
//                         eprintln!();
//                         eprintln!("Note: unable to get the compositor's version.");
//                         eprintln!("Did you forget to restart niri after an update?");
//                     }
//                     Err(_) => {
//                         // communication error, do not add irrelevant context
//                     }
//                 }
//             }

//             return Ok(());
//         }
//     };

//     match msg {
//         Msg::Error => {
//             bail!("unexpected response: expected an error, got {response:?}");
//         }
//         Msg::Version => {
//             let Response::Version(server_version) = response else {
//                 bail!("unexpected response: expected Version, got {response:?}");
//             };

//             if json {
//                 println!(
//                     "{}",
//                     json!({
//                         "cli": version(),
//                         "compositor": server_version,
//                     })
//                 );
//                 return Ok(());
//             }

//             let client_version = version();

//             println!("niri msg is {client_version}");
//             println!("the compositor is {server_version}");
//             if client_version != server_version {
//                 eprintln!();
//                 eprintln!("These are different");
//                 eprintln!("Did you forget to restart niri after an update?");
//             }
//             println!();
//         }
//         Msg::Outputs => {
//             let Response::Outputs(outputs) = response else {
//                 bail!("unexpected response: expected Outputs, got {response:?}");
//             };

//             if json {
//                 let output =
//                     serde_json::to_string(&outputs).context("error formatting response")?;
//                 println!("{output}");
//                 return Ok(());
//             }

//             let mut outputs = outputs.into_iter().collect::<Vec<_>>();
//             outputs.sort_unstable_by(|a, b| a.0.cmp(&b.0));

//             for (connector, output) in outputs.into_iter() {
//                 let Output {
//                     name,
//                     make,
//                     model,
//                     physical_size,
//                     modes,
//                     current_mode,
//                     logical,
//                 } = output;

//                 println!(r#"Output "{connector}" ({make} - {model} - {name})"#);

//                 if let Some(current) = current_mode {
//                     let mode = *modes
//                         .get(current)
//                         .context("invalid response: current mode does not exist")?;
//                     let Mode {
//                         width,
//                         height,
//                         refresh_rate,
//                         is_preferred,
//                     } = mode;
//                     let refresh = refresh_rate as f64 / 1000.;
//                     let preferred = if is_preferred { " (preferred)" } else { "" };
//                     println!("  Current mode: {width}x{height} @ {refresh:.3} Hz{preferred}");
//                 } else {
//                     println!("  Disabled");
//                 }

//                 if let Some((width, height)) = physical_size {
//                     println!("  Physical size: {width}x{height} mm");
//                 } else {
//                     println!("  Physical size: unknown");
//                 }

//                 if let Some(logical) = logical {
//                     let LogicalOutput {
//                         x,
//                         y,
//                         width,
//                         height,
//                         scale,
//                         transform,
//                     } = logical;
//                     println!("  Logical position: {x}, {y}");
//                     println!("  Logical size: {width}x{height}");
//                     println!("  Scale: {scale}");

//                     let transform = match transform {
//                         niri_ipc::Transform::Normal => "normal",
//                         niri_ipc::Transform::_90 => "90° counter-clockwise",
//                         niri_ipc::Transform::_180 => "180°",
//                         niri_ipc::Transform::_270 => "270° counter-clockwise",
//                         niri_ipc::Transform::Flipped => "flipped horizontally",
//                         niri_ipc::Transform::Flipped90 => {
//                             "90° counter-clockwise, flipped horizontally"
//                         }
//                         niri_ipc::Transform::Flipped180 => "flipped vertically",
//                         niri_ipc::Transform::Flipped270 => {
//                             "270° counter-clockwise, flipped horizontally"
//                         }
//                     };
//                     println!("  Transform: {transform}");
//                 }

//                 println!("  Available modes:");
//                 for (idx, mode) in modes.into_iter().enumerate() {
//                     let Mode {
//                         width,
//                         height,
//                         refresh_rate,
//                         is_preferred,
//                     } = mode;
//                     let refresh = refresh_rate as f64 / 1000.;

//                     let is_current = Some(idx) == current_mode;
//                     let qualifier = match (is_current, is_preferred) {
//                         (true, true) => " (current, preferred)",
//                         (true, false) => " (current)",
//                         (false, true) => " (preferred)",
//                         (false, false) => "",
//                     };

//                     println!("    {width}x{height}@{refresh:.3}{qualifier}");
//                 }
//                 println!();
//             }
//         }
//         Msg::FocusedWindow => {
//             let Response::FocusedWindow(window) = response else {
//                 bail!("unexpected response: expected FocusedWindow, got {response:?}");
//             };

//             if json {
//                 let window = serde_json::to_string(&window).context("error formatting
// response")?;                 println!("{window}");
//                 return Ok(());
//             }

//             if let Some(window) = window {
//                 println!("Focused window:");

//                 if let Some(title) = window.title {
//                     println!("  Title: \"{title}\"");
//                 } else {
//                     println!("  Title: (unset)");
//                 }

//                 if let Some(app_id) = window.app_id {
//                     println!("  App ID: \"{app_id}\"");
//                 } else {
//                     println!("  App ID: (unset)");
//                 }
//             } else {
//                 println!("No window is focused.");
//             }
//         }
//         Msg::Action { .. } => {
//             let Response::Handled = response else {
//                 bail!("unexpected response: expected Handled, got {response:?}");
//             };
//         }
//     }

//     Ok(())
// }

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
