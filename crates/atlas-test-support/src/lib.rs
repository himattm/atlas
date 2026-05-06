use anyhow::Result;
use serde_json::Value;
use std::process::Command;

pub struct CommandSnapshot {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_command(command: &mut Command) -> Result<CommandSnapshot> {
    let output = command.output()?;
    Ok(CommandSnapshot {
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub fn parse_json(stdout: &str) -> Result<Value> {
    Ok(serde_json::from_str(stdout)?)
}
