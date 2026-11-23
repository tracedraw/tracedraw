#!/usr/bin/env python3
"""Measure an empty GUI against fake lpstat inside a memory-limited container."""
import argparse
import csv
import json
import os
from pathlib import Path
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument("--label", required=True)
p.add_argument("--seconds", type=float, default=20)
p.add_argument("--rss-stop-mib", type=int, default=384)
p.add_argument("--compact-control", action="store_true")
p.add_argument("--figures", action="store_true")
a = p.parse_args()
result = Path("/results") / a.label
result.mkdir(parents=True, exist_ok=True)
home = Path("/tmp/printer-query-home")
config = home / "config/tracedraw"
config.mkdir(parents=True)
(config / "settings.json").write_text(json.dumps({
    "startup": "welcome_screen", "check_updates": False,
    "update_check_asked": True, "ai_enabled": False,
    "show_welcome_on_start": True,
}))
env = os.environ.copy()
env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / "config"),
           XDG_RUNTIME_DIR="/tmp/runtime", DISPLAY=":99",
           LIBGL_ALWAYS_SOFTWARE="1", FAKE_LPSTAT_ARGS_LOG="/tmp/lpstat-args.jsonl")
Path(env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
prefix = "/opt/fake/bin"
if a.compact_control:
    # This control uses the unmodified official GUI with the same
    # argument substitution as the separately tested local workaround.
    # It is not a build of the Rust fix in this PR.
    # Docker may mount /tmp with noexec; this PATH entry must be executable.
    guard = Path("/opt/control-bin")
    guard.mkdir()
    program = guard / "lpstat"
    program.write_text("#!/usr/bin/python3\nimport os,sys\n"
                       "args=[x for x in sys.argv[1:] if x!='-l']\n"
                       "os.execv('/opt/fake/bin/lpstat',['lpstat',*args])\n")
    program.chmod(0o755)
    prefix = str(guard) + os.pathsep + prefix
env["PATH"] = prefix + os.pathsep + env.get("PATH", os.defpath)
server = None
app = None
rows = []
stop = "time-limit"
try:
    with (result / "application.log").open("w") as log, (result / "memory.csv").open("w") as data:
        server = subprocess.Popen(["Xvfb", ":99", "-screen", "0", "1280x900x24"],
                                  stdout=log, stderr=log, env=env)
        time.sleep(0.5)
        args = ["/opt/tracedraw/tracedraw", "--renderer=software", "--no-splash"]
        if a.figures:
            args.append("/opt/green-triangle-line.svg")
        app = subprocess.Popen(args, stdout=log, stderr=log, env=env)
        start = time.monotonic()
        writer = csv.writer(data)
        writer.writerow(["seconds", "rss_kib", "anon_kib"])
        while time.monotonic() - start < a.seconds:
            if app.poll() is not None:
                stop = "process-exited"
                break
            status = {}
            try:
                for line in Path(f"/proc/{app.pid}/status").read_text().splitlines():
                    key, _, value = line.partition(":")
                    words = value.split()
                    if words and words[0].isdigit():
                        status[key] = int(words[0])
            except FileNotFoundError:
                stop = "process-exited"
                break
            row = [round(time.monotonic() - start, 3),
                   status.get("VmRSS", 0), status.get("RssAnon", 0)]
            rows.append(row)
            writer.writerow(row)
            data.flush()
            if row[1] >= a.rss_stop_mib * 1024:
                stop = "protective-rss-limit"
                break
            time.sleep(0.1)
finally:
    for process in [app, server]:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
events = Path("/sys/fs/cgroup/memory.events").read_text().strip()
summary = {
    "label": a.label, "stop": stop,
    "seconds": rows[-1][0] if rows else 0,
    "peak_rss_mib": round(max((r[1] for r in rows), default=0) / 1024, 2),
    "compact_control": a.compact_control, "synthetic_figures": a.figures,
    "app_exit": app.returncode if app is not None else None,
    "query_arguments": [json.loads(line) for line in Path("/tmp/lpstat-args.jsonl").read_text().splitlines()] if Path("/tmp/lpstat-args.jsonl").exists() else [],
    "memory_events": events,
}
(result / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary), flush=True)
