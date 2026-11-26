#!/usr/bin/env python3
"""Generate an original, deterministic two-page layer-drag review drawing.

Run from the repository root: python3 scripts/samples/layer_drag.py
The drawing and generator are licensed MIT OR Apache-2.0.
"""
import gzip
import json
from pathlib import Path


def rectangle(identifier, name, offset, rgb):
    return {
        "id": identifier, "name": name,
        "kind": {"type": "rect", "rect": {
            "x0": offset, "y0": offset, "x1": offset + 60, "y1": offset + 45,
        }, "radius": 0},
        "transform": [1, 0, 0, 1, 0, 0],
        "fill": {"fill": "solid", "model": "rgb", **dict(zip(("r", "g", "b"), rgb))},
        "stroke": None, "visible": True, "locked": False,
    }


def layer(identifier, name, shapes=(), **flags):
    return {
        "id": identifier, "name": name, "visible": True,
        "locked": False, "printable": True, "shapes": list(shapes), **flags,
    }


drawing = {
    "title": "Layer dragging", "ids": {"next": 20},
    "pages": [
        {"id": 1, "name": "Page 1", "size": {"width": 160, "height": 120},
         "layers": [
             layer(2, "Bottom - blue", [rectangle(3, "Blue rectangle", 20, (0.1, 0.3, 0.9))]),
             layer(4, "Middle - green", [rectangle(5, "Green rectangle", 35, (0.1, 0.7, 0.3))]),
             layer(6, "Top - orange", [rectangle(7, "Orange rectangle", 50, (0.95, 0.5, 0.1))]),
         ]},
        {"id": 8, "name": "Page 2", "size": {"width": 160, "height": 120},
         "layers": [layer(9, "Destination")]},
    ],
    "master": [
        layer(10, "Desktop", desktop=True),
        layer(11, "Shared A"), layer(12, "Shared B", scope="odd", in_front=True),
        layer(13, "Range A", page_range=[{"first": 1, "last": 2}]),
        layer(14, "Range B", page_range=[{"first": 1, "last": 2}]),
    ],
}
output = Path("docs/examples/layer-drag/layer-drag.tdraw")
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(gzip.compress(json.dumps(drawing, separators=(",", ":")).encode(), mtime=0))
print(output)
