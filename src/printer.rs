use std::{
    collections::HashMap,
    io::{stdout, Write},
};

use crate::{
    cli::{Color, TracingArgs},
    state::ProcessState,
};

use owo_colors::OwoColorize;

pub fn print_execve_trace(
    state: &ProcessState,
    result: i64,
    tracing_args: &TracingArgs,
    env: &HashMap<String, String>,
    color: Color,
) -> color_eyre::Result<()> {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some
    let exec_data = state.exec_data.as_ref().unwrap();
    let mut stdout = stdout();
    write!(stdout, "{}", state.pid.yellow())?;
    let trace_comm = !tracing_args.no_trace_comm;
    let trace_argv = !tracing_args.no_trace_argv;
    let trace_env = tracing_args.trace_env;
    let diff_env = !tracing_args.no_diff_env && !trace_env;
    let trace_filename = !tracing_args.no_trace_filename;
    if trace_comm {
        write!(stdout, "<{}>", state.comm.cyan())?;
    }
    write!(stdout, ":")?;
    if trace_filename {
        write!(stdout, " {:?}", exec_data.filename)?;
    }
    if trace_argv {
        write!(stdout, " {:?}", exec_data.argv)?;
    }
    if diff_env {
        // TODO: make it faster
        //       This is mostly a proof of concept
        write!(stdout, " [")?;
        let mut env = env.clone();
        for item in exec_data.envp.iter() {
            let (k, v) = {
                let mut sep_loc = item
                    .as_bytes()
                    .iter()
                    .position(|&x| x == b'=')
                    .unwrap_or_else(|| {
                        log::warn!(
                            "Invalid envp entry: {:?}, assuming value to empty string!",
                            item
                        );
                        item.len()
                    });
                if sep_loc == 0 {
                    // Find the next equal sign
                    sep_loc = item.as_bytes().iter().skip(1).position(|&x| x == b'=').unwrap_or_else(|| {
                        log::warn!("Invalid envp entry staring with '=': {:?}, assuming value to empty string!", item);
                        item.len()
                    });
                }
                let (head, tail) = item.split_at(sep_loc);
                (head, &tail[1..])
            };
            // Too bad that we still don't have if- and while-let-chains
            // https://github.com/rust-lang/rust/issues/53667
            if let Some(orig_v) = env.get(k).map(|x| x.as_str()) {
                if orig_v != v {
                    write!(
                        stdout,
                        "{}{:?}={:?}, ",
                        "M".bright_yellow().bold(),
                        k,
                        v.on_blue()
                    )?;
                }
                // Remove existing entry
                env.remove(k);
            } else {
                write!(
                    stdout,
                    "{}{:?}{}{:?}, ",
                    "+".bright_green().bold(),
                    k.on_green(),
                    "=".on_green(),
                    v.on_green()
                )?;
            }
        }
        // Now we have the tracee removed entries in env
        for (k, v) in env.iter() {
            write!(
                stdout,
                "{}{:?}{}{:?}, ",
                "-".bright_red().bold(),
                k.on_red().strikethrough(),
                "=".on_red().strikethrough(),
                v.on_red().strikethrough()
            )?;
        }
        write!(stdout, "]")?;
        // Avoid trailing color
        // https://unix.stackexchange.com/questions/212933/background-color-whitespace-when-end-of-the-terminal-reached
        if owo_colors::control::should_colorize() {
            write!(stdout, "\x1B[49m\x1B[K")?;
        }
    } else if trace_env {
        write!(stdout, " {:?}", exec_data.envp)?;
    }
    if result == 0 {
        writeln!(stdout)?;
    } else {
        let decode_errno = !tracing_args.no_decode_errno;
        if decode_errno {
            writeln!(
                stdout,
                " = {} ({})",
                result,
                nix::errno::Errno::from_i32(-result as i32)
            )?;
        } else {
            writeln!(stdout, " = {} ", result)?;
        }
    }
    Ok(())
}