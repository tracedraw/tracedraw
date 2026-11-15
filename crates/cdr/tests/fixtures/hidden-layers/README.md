# Layer visibility (#2862)

Original CDR27 parser samples made for TraceDraw, MIT OR Apache-2.0.
Run `python3 generate.py` to regenerate the deterministic `samples.zip`.
The Python generator needs only the standard library.

The flags were observed in a contributor-owned version 27 drawing:
`00 00 00 98` for a visible ordinary layer, `40 01 00 98` for a hidden one.
The reporter's screenshot distinguishes their visibility. Byte-0 flag
`0x40` is present on the ordinary layer, not a structural layer type;
its other meaning has not been established. Byte-1 bit 0 is read as the
hidden state on modern layer records. Version 27 was checked; earlier
versions have not been independently verified by this sample.

Samples contain only original black rectangles and generated chunk records.
No source drawing, text, metadata, fonts, pictures or screenshots are included.
These are parser fixtures, not files saved or validated by another editor.

| Sample | Expected |
| --- | --- |
| `two-layers.cdr` | VISIBLE on, HIDDEN off; both rectangles retained with their own visibility on |
| `hidden-only.cdr` | One hidden layer, retained; renders blank |
| `hidden-empty.cdr` | Empty hidden ordinary layer retained beside VISIBLE |
| `hidden-master.cdr` | Hidden master layer retained, ordinary page unaffected |
| `visible.cdr` | Visible control |
| `missing-flags.cdr` | Missing flags leave layer visible |
| `short-flags.cdr` | Truncated flags ignored safely |
| `wrong-record-type.cdr` | Non-layer record type ignored |
| `unrelated-bit.cdr` | Byte-1 bit 1 does not hide a layer |
| `guides.cdr` | Guide layer remains hidden and unprintable |

Native save/reopen preserves layer visibility. CDR export retention of
hidden layers is an existing separate limitation, outside this import fix.
