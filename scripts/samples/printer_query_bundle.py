#!/usr/bin/env python3
"""Build the deterministic, source-readable synthetic printer-query bundle."""
import hashlib
from pathlib import Path
import zipfile

root = Path(__file__).resolve().parents[2]
fixture = root / "docs/examples/printer-query"
paths = [root / "apps/tracedraw/src/printer_query.rs",
         root / "LICENSE-MIT", root / "LICENSE-APACHE"]
for path in fixture.rglob("*"):
    if not path.is_file() or any(p in {"target", "__pycache__"} for p in path.parts):
        continue
    if path.name == "Dockerfile" or path.suffix in {".py", ".md", ".csv", ".json", ".svg", ".toml", ".lock"}:
        paths.append(path)
paths = sorted(paths, key=lambda p: p.relative_to(root).as_posix())
archive = root / "docs/examples/printer-query.zip"
checksums = []
with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
    for path in paths:
        name = path.relative_to(root).as_posix()
        data = path.read_bytes()
        info = zipfile.ZipInfo(name, date_time=(2024, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = (0o100755 if path.suffix == ".py" else 0o100644) << 16
        bundle.writestr(info, data, compresslevel=9)
        checksums.append(hashlib.sha256(data).hexdigest() + "  " + name)
    info = zipfile.ZipInfo("SHA256SUMS", date_time=(2024, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    bundle.writestr(info, "\n".join(checksums) + "\n", compresslevel=9)
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix(".zip.sha256").write_text(digest + "  printer-query.zip\n")
print(f"{len(paths)} files; {archive.stat().st_size} bytes; SHA-256 {digest}")
