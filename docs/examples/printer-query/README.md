# Bounded printer discovery (#3883)

Original synthetic fixtures and selected, document-independent measurements,
licensed MIT OR Apache-2.0. No real printer names, source drawings, raw
application logs, host paths, allocation trace files or memory dumps are
included. The triangle and line were drawn from scratch for this test.

## Confirmed startup allocation

TraceDraw 0.51.0 starts `lpstat -l -p -d` on a background thread and captures
its output using `Command::output()`. A repeating printer-status response
therefore grows an output buffer without limit, even before a document is
opened. The observed verbose query produced 33,558,528 bytes in 0.220 s and
was stopped externally. The compact `lpstat -p -d` query completed in
0.031 s with 435 bytes. Those counts were selected manually; no printer
response contents are published.

The selected allocation call chain was:

```text
App::fetch_printers background worker
printers::system_printers
std::process::Command::output
std::sys::fd::unix::FileDesc::read_to_end
alloc::raw_vec::RawVecInner::finish_grow
```

The printer output buffer accounted for approximately 512 MiB of live
requested allocations in a 5.7-second capture. This is a heap allocation
measurement, distinct from process RSS. No claim is made that every older
OOM incident in #3883 had this same cause.

## Safe synthetic reproduction with the official GUI

Do not put the fake utility in an unrestricted desktop PATH. Run it only in
the bounded container below. It never contacts a print server. Its verbose
mode repeats synthetic text; its compact mode returns a finite list.

From the repository root, with an official Linux x86_64 release extracted
into `RELEASE_DIR` (containing `tracedraw` and `tracedraw-cli`):

```sh
docker build -t tracedraw-printer-reproducer docs/examples/printer-query
mkdir -p /tmp/tracedraw-printer-results
docker run --rm --network=none --memory=768m --memory-swap=768m --cpus=2 \
  --pids-limit=128 --tmpfs /tmp:rw,size=128m \
  -v "$RELEASE_DIR:/opt/tracedraw:ro" \
  -v /tmp/tracedraw-printer-results:/results \
  tracedraw-printer-reproducer --label repeating-query
docker run --rm --network=none --memory=768m --memory-swap=768m --cpus=2 \
  --pids-limit=128 --tmpfs /tmp:rw,size=128m \
  -v "$RELEASE_DIR:/opt/tracedraw:ro" \
  -v /tmp/tracedraw-printer-results:/results \
  tracedraw-printer-reproducer --label compact-control --compact-control --figures
```

No host home directory, drawings, display, GPU or Docker socket is mounted.
Equal memory/swap limits disable swap. The watchdog stops at 384 MiB RSS
or 20 seconds, before the 768 MiB hard limit in the recorded cases. It sends
SIGTERM to the synthetic test instance, with SIGKILL as a fallback; an
`app_exit` of -15 is that deliberate termination, not an OOM kill. Inspect
`memory_events` for `oom`/`oom_kill`. An OOM kill is a failed safe test.

The compact control uses an argument wrapper with the unmodified official
GUI. It demonstrates the root cause and matches the local compatibility
workaround; it is **not** a GUI build of this PR. To test a built PR binary,
mount its directory and omit `--compact-control`: the production helper
should bound the verbose response and fall back by itself.

## Focused production-helper regressions

`harness/Cargo.toml` compiles the actual `printer_query.rs` source, including
its tests, without building the application. Use Rust 1.97.0. Build in a
memory-limited container, then run the prebuilt tests with networking off:

```sh
cargo test --manifest-path docs/examples/printer-query/harness/Cargo.toml \
  --locked --offline --lib -- --test-threads=2
cargo clippy --manifest-path docs/examples/printer-query/harness/Cargo.toml \
  --locked --offline --lib --tests -- -D warnings
```

Offline commands require the one pinned dependency to have been fetched
during the separate build step. The tests cover complete/exact-limit
output, repeating output, ignored stderr, deadlines, child reaping,
inherited pipes, exit/spawn failures, preservation of verbose metadata,
and compact fallback after excessive output or a stuck verbose query.
No tests are removed. Broad local application/workspace suites were not
run or split into batches; they remain for upstream CI.

`harness/examples/query.rs` also runs the production helper against
`fake-lpstat.py` and reports only byte counts, elapsed time and process
memory. The fixture's `FAKE_LPSTAT_VERBOSE` variable accepts `repeat`
(default), `hang`, or `valid`. The first two should return a compact
list after the corresponding bound; `valid` should retain its verbose
description and location. `measurements/helper-controls.json` records
the Linux container results. This is a test of the actual Rust helper,
separate from the official-GUI control above.

## Recorded measurements

`measurements/` contains selected CSV columns only: elapsed time, phase,
process RSS and anonymous RSS, all in KiB. Exact wall-clock times, system
memory figures and process identifiers were removed.

The host comparisons used the unmodified official 0.51.0 binary in a
1.5 GiB/no-swap/two-CPU systemd service, with fresh test settings and a
watchdog that stopped at 768 MiB process RSS. The patched Rust helper was
tested separately through the harness. After the local compact-query
guard, the GUI stayed around 375 MiB with heaptrack, 289 MiB without it
on hardware rendering, and 294 MiB with the original triangle/line scene
and mouse/panel activity, each for about 85 seconds. The unguarded capture
crossed the watchdog threshold in about six seconds. Profiler overhead
means these RSS figures are not directly interchangeable.

The initial Docker checks had no `lpstat` installed and skipped the failing
path. The fake utility makes that missing condition reproducible without
access to a real printer or private artwork. Full application builds and
broad suites are left to upstream CI; these assets do not claim otherwise.

The download bundle is rebuilt from these readable files and the actual
production helper with `python3 scripts/samples/printer_query_bundle.py`.
It contains repository-relative paths, both repository licenses and an
internal SHA256SUMS list. No executable application or raw trace is bundled.
