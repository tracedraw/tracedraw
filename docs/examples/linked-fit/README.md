# Live fit save and reopen samples

These original drawings exercise an existing live Fit Objects to Path,
including the case of a linked brush stroke adopted on import. They use
only an original triangular marker and an open guide with line and cubic
segments. No external clipart, fonts or embedded motif library is included.
The source is `fit_live::tests::stroke_drawing` and
`fit_live::tests::write_linked_fit_review_samples`.

- `linked-fit-original`: 5 mm spacing, 180 degree path-relative turn.
- `linked-fit-edited`: the same guide scaled and moved, with 10 mm spacing.
- `.tdraw` retains the live fit in the native format.
- `.cdr` is a version 27 package. It retains the live fit when reopened
  in TraceDraw through `tracedraw/data.json`. Other readers see placed
  vectors. These files do not demonstrate native linked brush export.
- `.svg` and `.png` show each drawing as ordinary vector artwork and a render.

Open the original `.tdraw`, select the guide, save as a version 27 `.cdr`,
close it and reopen the saved file. With the fix, reshaping or moving the
guide moves its markers. Object > Fit Objects to Path also retains the
spacing and turn. Change spacing, then undo. The fit should remain live,
without duplicate or missing markers. The edited sample shows one possible
result without redrawing the guide.

Official Linux v0.52.0 loses all 30 stable identities on the original
sample's `.tdraw` to `.cdr` to `.tdraw` conversion while retaining the
`fit_path.live` settings. The stale references cannot find their guide or
members. The regression covers two save/reopen cycles, movement, spacing
and undo, in package versions 14, 17 and 27.

Regenerate the drawings, rather than changing the generated files by hand:

```sh
TRACEDRAW_FIT_SAMPLE_DIR="$PWD/docs/examples/linked-fit" \
  cargo test -p tracedraw --lib write_linked_fit_review_samples -- --ignored --nocapture
```

Only this explicitly requested sample generator writes files. Ordinary
live-fit tests do not. For a resource-limited Docker run, reuse the
repository's available Rust 1.97.0 development image and caches, cap it at
two CPUs and 6 GiB, and pass the output variable into the container.

The upstream CDR writer/checker reports 15 bounding-box findings for the
original sample. Official v0.52.0 reports the same findings on its export;
the identity metadata change does not alter the vector records. That
separate writer/checker concern is outside this save/reopen fix.

All geometry and generated assets here are by TraceDraw contributors,
licensed MIT OR Apache-2.0, like the repository. `linked-fit-samples.zip`
contains this README and both sets of files for attaching to a report.
