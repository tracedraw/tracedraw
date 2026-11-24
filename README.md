<p align="center">
  <img src="assets/icon/tracedraw-256.png" alt="TraceDraw" width="120">
</p>

<h1 align="center">TraceDraw + plugins (personal fork)</h1>

<p align="center">
  A personal build of <a href="https://github.com/tracedraw/tracedraw">TraceDraw</a>, the open-source vector editor,
  with two things added on top (Windows only), neither part of the real TraceDraw.
</p>

## What this fork adds

- **External panel** (`docs/behavior/external-panel.md`) — another program
  can show one of its own windows docked as a panel in TraceDraw's window,
  through the open window's MCP link. Also proposed upstream as
  [tracedraw/tracedraw#4188](https://github.com/tracedraw/tracedraw/pull/4188).
- **Native plugins** (`docs/behavior/native-plugins.md`) — Window > Plugins
  > Import Plugin installs one from a `.zip` (a `plugin.toml` naming the
  program and the link that opens it); opening it docks the program as a
  panel, and it opens itself again on its own the next time TraceDraw
  starts, like a docker that stays open. Fork-only: a native plugin has
  whatever access the other program has, unlike TraceDraw's own sandboxed
  Tools > Plugins.

## Download

The latest build: [`native-plugins-latest`](https://github.com/SnakyWarrior/tracedraw/releases/tag/native-plugins-latest)
— one file, `tracedraw.exe`. No installer: it replaces the same-named
file wherever TraceDraw is already installed (same shortcuts, file
associations and uninstaller), or is the whole install on its own.

## Keeping it current

`tools/update-fork.ps1` rebases both features onto a new upstream release
and rebuilds; `.github/workflows/update-fork.yml` does the same on a
schedule and only pushes and republishes the release on a clean result
(a conflicting upstream change stops it, nothing broken pushed).

## Everything else

This is still TraceDraw: the real project's own documentation
(`docs/`), license (MIT or Apache-2.0) and contributing guide
(`CONTRIBUTING.md`) apply. See the
[original README](https://github.com/tracedraw/tracedraw#readme) for
TraceDraw itself — its features, building it, and how to contribute
there.
