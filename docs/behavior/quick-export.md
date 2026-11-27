# Quick Export

Window > Panels > Quick Export keeps common image export choices beside the
drawing. Choose a file name and a format on their own rows. Use the button
beside the file name to take it from the drawing. An empty name uses the
file name of the drawing or `drawing` for an unsaved document. On the desktop,
type a folder path and press Enter, or choose it with Choose folder. Open
Folder opens the destination in the system file manager. The path menu shows
the six most recently used folders and remembers them across restarts. Use the
cross beside an entry to remove it from the list. The folder and its files
stay on disk.

The available formats are PNG, JPEG, BMP, TIFF, ICO, WebP, PDF, SVG and PSD.
The defaults are PNG at 100 dpi, scale 1, a transparent background,
anti-aliasing and Selected area. The separate objects checkbox is on by
default. For bitmap formats with resolution and scale controls, type values
or pick them from the lists. Enter or leaving the field applies the value.
The effective resolution is dpi times scale, bounded to 1 through 2400 dpi.
Multiply each side in millimetres by the effective resolution, divide by
25.4 and round up to get the pixel dimensions. Colour and transparency controls appear for
supported formats. JPEG and TIFF offer
RGB, CMYK and grayscale. PDF keeps the original colours by default. Its
CMYK option writes process colours for print and keeps transparency and
spot inks. PNG offers RGB, grayscale, indexed colour and black and white.

ICO renders the artwork into a square icon at 16, 24, 32, 48, 64, 128 or
256 pixels per side. The default is 256 pixels. The content is
scaled uniformly to fit and centred. Transparent background preserves
alpha, including in the padding, and Smooth edges controls anti-aliasing.

The separate objects checkbox sits with the transparency and smoothing
options when the format offers those controls, above the export range.
Pick one range (Selected area, Current canvas or All pages). Selected area
exports the whole current artboard when the selection is empty. It stays
selected, so the next export uses the selection as soon as objects are
selected. The separate objects checkbox keeps its setting when the range
changes. With that checkbox on, Current canvas and All pages include visible
top-level objects on visible, printable layers of the pages in range. A group exports as one object. Each
object is cropped to its visual bounds, including its outline.

For PDF, One page per object puts all of the objects into one PDF, with one
object per page. Other formats offer One file per object. On the desktop the
files go into the chosen folder. The browser and Android save them together
in a ZIP file. If there are no exportable objects in the range, Quick Export
reports it. Clear the checkbox to export the selection together or use the
original page layout.

On the desktop, Overwrite same name is off by default. When it is off,
Quick Export adds ` (2)`, ` (3)` and so on before the extension if a file
already has that name. Turn it on to replace existing files at the export
paths. Objects with the same name in a single export still get separate files.

Export uses the current panel settings. The panel has no dedicated keyboard
shortcut. The Export panel also offers asset lists, slices and presets.
