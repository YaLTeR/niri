use std::env;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use anyhow::{anyhow, bail, Context};
use niri_ipc::{Mode, Output, Reply, Request, Response};

use crate::cli::Msg;

pub fn handle_msg(msg: Msg, json: bool) -> anyhow::Result<()> {
    let socket_path = env::var_os(niri_ipc::SOCKET_PATH_ENV).with_context(|| {
        format!(
            "{} is not set, are you running this within niri?",
            niri_ipc::SOCKET_PATH_ENV
        )
    })?;

    let mut stream =
        UnixStream::connect(socket_path).context("error connecting to {socket_path}")?;

    let request = match &msg {
        Msg::Outputs => Request::Outputs,
        Msg::Action { action } => Request::Action(action.clone()),
    };
    let mut buf = serde_json::to_vec(&request).unwrap();
    stream
        .write_all(&buf)
        .context("error writing IPC request")?;
    stream
        .shutdown(Shutdown::Write)
        .context("error closing IPC stream for writing")?;

    buf.clear();
    stream
        .read_to_end(&mut buf)
        .context("error reading IPC response")?;

    let reply: Reply = serde_json::from_slice(&buf).context("error parsing IPC reply")?;

    let response = reply
        .map_err(|msg| anyhow!(msg))
        .context("niri could not handle the request")?;

    match msg {
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
                    } = mode;
                    let refresh = refresh_rate as f64 / 1000.;
                    println!("  Current mode: {width}x{height} @ {refresh:.3} Hz");
                } else {
                    println!("  Disabled");
                }

                if let Some((width, height)) = physical_size {
                    println!("  Physical size: {width}x{height} mm");
                } else {
                    println!("  Physical size: unknown");
                }

                println!("  Available modes:");
                for mode in modes {
                    let Mode {
                        width,
                        height,
                        refresh_rate,
                    } = mode;
                    let refresh = refresh_rate as f64 / 1000.;
                    println!("    {width}x{height}@{refresh:.3}");
                }
                println!();
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
