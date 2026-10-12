//! A minimal success/failure ping to a local pulse agent
//! (<https://github.com/julecko/pulse>), via `pulse-agent-cli notify`. Opt-in
//! via `PULSE_NOTIFY=true` in `.env` - unset or any other value stays
//! silent, so a machine without pulse installed doesn't need to care. Never
//! fails the run: a missing binary or a pulse-side error is only logged.

use std::env;
use std::process::Command;

const BINARY: &str = "pulse-agent-cli";
/// Appended to the host's own name by pulse-agent-cli, e.g. "web01: jumakl-extractor".
const TITLE: &str = "jumakl-extractor";

pub fn notify_run_result(failed: bool) {
    if env::var("PULSE_NOTIFY").as_deref() != Ok("true") {
        return;
    }

    let message = if failed {
        "Run failed"
    } else {
        "Run succeeded"
    };

    // The agent's socket only accepts root or the agent's own user - see
    // https://github.com/julecko/pulse#notifications-from-a-host.
    match Command::new("sudo")
        .args([BINARY, "notify", "-t", TITLE, message])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!("{BINARY} notify exited with {status}"),
        Err(err) => tracing::warn!("failed to run {BINARY}: {err:#}"),
    }
}
