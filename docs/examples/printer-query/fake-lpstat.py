#!/usr/bin/env python3
"""Original synthetic printer-query fixture; never contacts a print server."""
import os
import json
import sys
import time

verbose = "-l" in sys.argv[1:]
if os.environ.get("FAKE_LPSTAT_ARGS_LOG"):
    with open(os.environ["FAKE_LPSTAT_ARGS_LOG"], "a") as log:
        log.write(json.dumps(sys.argv[1:]) + "\n")
mode = os.environ.get("FAKE_LPSTAT_VERBOSE", "repeat")
compact = (
    b"printer Sample_Printer is idle. enabled since Mon 01 Jan 2024 00:00:00\n"
    b"system default destination: Sample_Printer\n"
)
details = b"\tDescription: Synthetic printer\n\tLocation: Synthetic room\n"
if verbose and mode == "repeat":
    block = (compact + details) * 32
    try:
        while True:
            sys.stdout.buffer.write(block)
            sys.stdout.buffer.flush()
    except BrokenPipeError:
        os._exit(0)
elif verbose and mode == "hang":
    time.sleep(30)
else:
    sys.stdout.buffer.write(compact + (details if verbose else b""))
