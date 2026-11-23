//! Run the production printer-query helper against a synthetic executable.
use std::io;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = std::env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::other("expected a synthetic lpstat executable"))?;
    let start = Instant::now();
    let bytes =
        tracedraw_printer_query_regressions::lpstat(program, 1024 * 1024, Duration::from_secs(5))?;
    let peak = std::fs::read_to_string("/proc/self/status")?
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| io::Error::other("expected Linux process memory accounting"))?;
    println!(
        "{{\"stdout_bytes\":{},\"elapsed_ms\":{},\"preserved_description\":{},\"peak_rss_kib\":{}}}",
        bytes.len(),
        start.elapsed().as_millis(),
        String::from_utf8_lossy(&bytes).contains("Description:"),
        peak,
    );
    Ok(())
}
