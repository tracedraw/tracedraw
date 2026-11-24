# Native plugins (fork-only)

**Never proposed upstream.** Unlike Tools > Plugins (`plugins.md`), whose
scripts run in a JavaScript sandbox with no access to the disk or the
network, a native plugin is another program on this computer: it has
whatever access that program has. This is normal for a tool installed on
purpose, and it is why this feature stays in this fork and is never
offered as a general install-anything mechanism in the upstream project.

## The manifest

Window > Plugins > Import Plugin installs a `.zip` holding one file,
`plugin.toml`, in the simple `key = "value"` format the language files
already use (`i18n::parse`):

```toml
name = "My Program"
launch = "myprogram://open/"
```

- `name`: shown in the menu and the Plugin Manager.
- `launch`: a URI already registered on this computer by the other
  program's own installer (a CorelDRAW add-on docker with a link of its
  own is the model: the program owns installing and running itself,
  TraceDraw never bundles or manages its executable).

Installing keeps only these two fields (`native_plugins.rs`,
`settings.json`'s `native_plugins`); nothing from the zip is extracted to
disk.

## Opening a plugin

Window > Plugins > a plugin's name: turns on Options > Global > General
> "Let AI agents on this computer edit the open drawing (MCP)" if it is
off, then (once the link exists, `poll_native_plugin_launch`, up to a
frame later) opens `launch` with the link's port and token added as
query parameters:

    myprogram://open/?td-port=51000&td-token=<token>

(`&` instead of `?` when `launch` already has a query string.) The
program on the other end reads them and calls the MCP tool `dock_window`
itself (`external-panel.md`), on its own time; TraceDraw does not wait
for it or track whether it ran.

Window > Plugins > Manage Plugins lists every installed plugin with its
`launch` URI and a Remove button.

## Opens again on its own next time

Like a docker that stays open across restarts: opening a plugin keeps its
name in `settings.json`'s `docked_native_plugin`, and the next time
TraceDraw starts it is opened again on its own, the same way (the first
frame only, `App::native_plugin_restored`). Closing the panel on purpose
forgets it; TraceDraw quitting does not (there is no panel open to close
at that point, only a frame that never ran). A plugin still only
launching (no panel open yet, `App::native_plugin_panel_was_open` not
set) is never mistaken for one that was closed.

## Not done

- One plugin docked at a time (the external panel's own limit).
- No uninstall-and-forget for the other program: closing or removing the
  plugin here only closes its panel (`undock_window`); the program
  itself, and whatever it is doing, is its own.
- No icon, description or version in the manifest.
- Windows only, like the external panel it opens.
