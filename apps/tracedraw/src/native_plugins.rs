//! Native plugins (fork-only, never proposed upstream: see
//! `docs/behavior/native-plugins.md`). Window > Plugins installs a
//! `.zip` holding one file, `plugin.toml` (the simple `key = "value"`
//! format `i18n::parse` already reads for the language files):
//!
//! ```toml
//! name = "My Program"
//! launch = "myprogram://open/"
//! ```
//!
//! `launch` is a URI already registered on this computer by the other
//! program's own installer (`external-panel.md`'s Corel precedent: the
//! program, not TraceDraw, owns installing and running itself).
//! Clicking the plugin's name turns on the MCP window link (Options >
//! Global > General) if it is off, then opens `launch` with the link's
//! port and token appended as query parameters (`td-port`, `td-token`);
//! the program on the other end reads them and calls `dock_window`
//! itself (`external_panel.rs`), on its own time.

use crate::app::App;
use crate::i18n::{tr, trf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct NativePlugin {
    pub name: String,
    pub launch: String,
}

/// `plugin.toml` inside the zip at `path`.
fn read_manifest(path: &std::path::Path) -> Result<NativePlugin, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut text = String::new();
    {
        let mut entry = zip
            .by_name("plugin.toml")
            .map_err(|_| tr("status.native_plugin_no_manifest"))?;
        std::io::Read::read_to_string(&mut entry, &mut text).map_err(|e| e.to_string())?;
    }
    let fields = crate::i18n::parse(&text);
    let name = fields.get("name").filter(|s| !s.is_empty()).cloned();
    let launch = fields.get("launch").filter(|s| !s.is_empty()).cloned();
    match (name, launch) {
        (Some(name), Some(launch)) => Ok(NativePlugin { name, launch }),
        _ => Err(tr("status.native_plugin_invalid")),
    }
}

/// Open `uri` through the registered protocol handler. `ShellExecuteW`,
/// not `cmd /C start`: a `cmd` shell re-parses `&` and other
/// metacharacters in the command line, which `uri`'s own query string
/// needs literally (`?td-port=...&td-token=...`).
#[cfg(windows)]
fn open_uri(uri: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let mut wide: Vec<u16> = uri.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid, nul-terminated UTF-16 buffer for the
    // call's duration; the other string pointers are null, which
    // ShellExecuteW accepts.
    let code = unsafe {
        ShellExecuteW(
            0,
            std::ptr::null(),
            wide.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success, else an error code
    // (or a HINSTANCE-shaped failure value) in the low bits.
    if (code as isize) > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW: error {code}"))
    }
}

/// `launch` with the MCP window link's port and token added as query
/// parameters (`&` instead of `?` when `launch` already has a query
/// string).
fn plugin_uri(launch: &str, port: u16, token: &str) -> String {
    let sep = if launch.contains('?') { '&' } else { '?' };
    format!("{launch}{sep}td-port={port}&td-token={token}")
}

impl App {
    /// Window > Plugins > Import Plugin: a `.zip` chosen.
    pub fn install_native_plugin_file(&mut self) {
        crate::files::Dialog::new()
            .add_filter("Plugin (*.zip)", &["zip"])
            .pick_file(self, |app, path| match read_manifest(&path) {
                Ok(p) => app.install_native_plugin(p),
                Err(e) => app.status = format!("{}: {e}", path.display()),
            });
    }

    /// Register `p`, replacing one of the same name.
    pub fn install_native_plugin(&mut self, p: NativePlugin) {
        let name = p.name.clone();
        match self.settings.native_plugins.iter_mut().find(|q| q.name == p.name) {
            Some(q) => *q = p,
            None => self.settings.native_plugins.push(p),
        }
        self.settings.save();
        self.status = trf("status.native_plugin_installed", &[("name", &name)]);
    }

    pub fn remove_native_plugin(&mut self, name: &str) {
        self.settings.native_plugins.retain(|p| p.name != name);
        if self.settings.docked_native_plugin.as_deref() == Some(name) {
            self.settings.docked_native_plugin = None;
        }
        self.settings.save();
    }

    /// Window > Plugins > a plugin's name: opens the docker at once,
    /// showing "Opening..." (`external_panel::opening`, like Corel's own
    /// docker, empty until its program's window arrives) and asks for
    /// the MCP window link, then (once it exists) opens the plugin's
    /// `launch` URI with its port and token. The actual open waits for
    /// the link (`poll_native_plugin_launch`, called once a frame):
    /// turning the option on takes effect the next frame, not this one.
    /// Kept as `settings.docked_native_plugin`, so it docks again, on its
    /// own, the next time TraceDraw starts (same idea as a docker
    /// staying open), until it is closed on purpose.
    pub fn open_native_plugin(&mut self, name: &str) {
        let Some(p) = self
            .settings
            .native_plugins
            .iter()
            .find(|p| p.name == name)
            .cloned()
        else {
            return;
        };
        self.settings.mcp_window = true;
        if self.settings.docked_native_plugin.as_deref() != Some(name) {
            self.settings.docked_native_plugin = Some(name.to_string());
            self.settings.save();
        }
        crate::external_panel::opening(self, name);
        self.pending_native_plugin = Some(p);
    }

    /// Once a frame: the plugin that was docked when TraceDraw last
    /// closed, opened again (the first frame only); a plugin waiting for
    /// the MCP window link, opened once the link exists (or given up on
    /// if it failed to start); the panel closing on purpose (not just
    /// still launching) forgets it for next time.
    pub fn poll_native_plugin_launch(&mut self) {
        if !self.native_plugin_restored {
            self.native_plugin_restored = true;
            if let Some(name) = self.settings.docked_native_plugin.clone() {
                self.open_native_plugin(&name);
            }
        }
        let open = self.docker_place(crate::app::DockerTab::External).is_some();
        if open {
            self.native_plugin_panel_was_open = true;
        } else if self.native_plugin_panel_was_open && self.pending_native_plugin.is_none() {
            self.native_plugin_panel_was_open = false;
            if self.settings.docked_native_plugin.is_some() {
                self.settings.docked_native_plugin = None;
                self.settings.save();
            }
        }
        let Some(p) = self.pending_native_plugin.take() else {
            return;
        };
        let Some(link) = &self.mcp_link else {
            // Still starting, or the option failed to turn on: try once
            // more next frame, unless it was turned off meanwhile.
            if self.settings.mcp_window {
                self.pending_native_plugin = Some(p);
            }
            return;
        };
        let uri = plugin_uri(&p.launch, link.info.port, &link.info.token);
        #[cfg(windows)]
        match open_uri(&uri) {
            Ok(_) => self.status = trf("status.native_plugin_opened", &[("name", &p.name)]),
            Err(e) => {
                self.status = trf(
                    "status.native_plugin_launch_failed",
                    &[("name", &p.name), ("e", &e)],
                );
            }
        }
        #[cfg(not(windows))]
        {
            self.status = tr("status.native_plugin_windows_only");
        }
    }
}

/// Window > Plugins: each installed plugin by name, then Manage Plugins
/// and Import Plugin.
pub fn menu_items(app: &mut App, ui: &mut egui::Ui) {
    use crate::ui::menus::{item, item_label, sep};
    let plugins = app.settings.native_plugins.clone();
    for p in &plugins {
        if item_label(ui, &p.name, "", true) {
            app.open_native_plugin(&p.name);
        }
    }
    if !plugins.is_empty() {
        sep(ui);
    }
    if item(ui, "menu.window.plugins_manage", "", !plugins.is_empty()) {
        app.dialog = crate::ui::dialogs::Dialog::NativePlugins;
    }
    if item(ui, "menu.window.plugins_import", "", true) {
        app.install_native_plugin_file();
    }
}

/// Window > Plugins > Manage Plugins: every installed plugin with its
/// launch URI and a Remove button; Import Plugin below.
pub fn manager(app: &mut App, ctx: &egui::Context, close: &mut bool) {
    let mut remove: Option<String> = None;
    let mut install = false;
    crate::ui::dialogs::window(ctx, tr("dialog.native_plugins")).show(ctx, |ui| {
        ui.set_min_width(420.0);
        ui.label(
            egui::RichText::new(tr("dialog.native_plugins_hint"))
                .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        if app.settings.native_plugins.is_empty() {
            ui.label(tr("dialog.native_plugins_none"));
        }
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for p in &app.settings.native_plugins {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&p.name).strong());
                            ui.label(
                                egui::RichText::new(&p.launch)
                                    .color(crate::theme::Tokens::text_dim())
                                    .small(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tr("docker.remove")).clicked() {
                                remove = Some(p.name.clone());
                            }
                        });
                    });
                }
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            install = ui.button(tr("menu.window.plugins_import")).clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("dialog.close")).clicked() {
                    *close = true;
                }
            });
        });
    });
    if let Some(name) = remove {
        app.remove_native_plugin(&name);
    }
    if install {
        app.install_native_plugin_file();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `?` already in `launch` takes `&`, not a second `?` (which
    /// would make the first query string invalid).
    #[test]
    fn the_separator_follows_an_existing_query_string() {
        assert_eq!(
            plugin_uri("myprogram://open/", 51000, "abc"),
            "myprogram://open/?td-port=51000&td-token=abc"
        );
        assert_eq!(
            plugin_uri("myprogram://open/?x=1", 51000, "abc"),
            "myprogram://open/?x=1&td-port=51000&td-token=abc"
        );
    }

    fn zip_with_manifest(toml: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("td-plugin-test-{:p}.zip", toml.as_ptr()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("plugin.toml", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, toml.as_bytes()).unwrap();
        zip.finish().unwrap();
        path
    }

    /// `name` and `launch` round-trip through the zip.
    #[test]
    fn a_manifest_with_both_fields_installs() {
        let path = zip_with_manifest("name = \"My Program\"\nlaunch = \"myprogram://open/\"\n");
        let p = read_manifest(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(p.name, "My Program");
        assert_eq!(p.launch, "myprogram://open/");
    }

    /// A manifest missing either field is refused, not installed half
    /// empty.
    #[test]
    fn a_manifest_missing_a_field_is_refused() {
        let path = zip_with_manifest("name = \"My Program\"\n");
        let err = read_manifest(&path);
        std::fs::remove_file(&path).ok();
        assert!(err.is_err());
    }

    /// A zip with no `plugin.toml` is refused, not silently installed
    /// with empty fields.
    #[test]
    fn a_zip_without_a_manifest_is_refused() {
        let path = std::env::temp_dir().join("td-plugin-test-empty.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"hello").unwrap();
        zip.finish().unwrap();
        let err = read_manifest(&path);
        std::fs::remove_file(&path).ok();
        assert!(err.is_err());
    }

    /// Installing a plugin of the same name replaces it instead of
    /// duplicating it in the menu.
    #[test]
    fn installing_the_same_name_again_replaces_it() {
        let mut app = App::headless();
        app.install_native_plugin(NativePlugin {
            name: "My Program".into(),
            launch: "myprogram://open/".into(),
        });
        app.install_native_plugin(NativePlugin {
            name: "My Program".into(),
            launch: "myprogram://open2/".into(),
        });
        assert_eq!(app.settings.native_plugins.len(), 1);
        assert_eq!(app.settings.native_plugins[0].launch, "myprogram://open2/");
        app.remove_native_plugin("My Program");
        assert!(app.settings.native_plugins.is_empty());
    }

    /// A fresh start with a plugin remembered from last time opens it
    /// again on its own, the same frame (`restored` only runs once).
    #[test]
    fn a_remembered_plugin_opens_itself_on_the_first_frame() {
        let mut app = App::headless();
        app.settings.native_plugins.push(NativePlugin {
            name: "My Program".into(),
            launch: "myprogram://open/".into(),
        });
        app.settings.docked_native_plugin = Some("My Program".into());
        assert!(app.pending_native_plugin.is_none());
        app.poll_native_plugin_launch();
        assert_eq!(
            app.pending_native_plugin.as_ref().map(|p| p.name.as_str()),
            Some("My Program")
        );
        assert!(app.native_plugin_restored);
        // A second frame does not ask again once it already has (no
        // mcp_link in a headless app, so it would otherwise be retried
        // forever instead of once).
        app.pending_native_plugin = None;
        app.settings.docked_native_plugin = None;
        app.poll_native_plugin_launch();
        assert!(app.pending_native_plugin.is_none());
    }

    /// Closing the panel on purpose forgets it for next time; a plugin
    /// still only launching (no panel open yet) is not mistaken for one
    /// that was closed.
    #[test]
    fn closing_the_panel_forgets_it_for_next_time() {
        use crate::app::DockerTab;
        let mut app = App::headless();
        app.install_native_plugin(NativePlugin {
            name: "My Program".into(),
            launch: "myprogram://open/".into(),
        });
        app.open_native_plugin("My Program");
        assert_eq!(
            app.settings.docked_native_plugin.as_deref(),
            Some("My Program")
        );
        // Still launching, no panel open yet: not treated as closed.
        app.poll_native_plugin_launch();
        assert_eq!(
            app.settings.docked_native_plugin.as_deref(),
            Some("My Program")
        );
        // Now it is really docked, then closed on purpose.
        app.open_docker(DockerTab::External);
        app.poll_native_plugin_launch();
        app.close_docker(DockerTab::External);
        app.pending_native_plugin = None;
        app.poll_native_plugin_launch();
        assert_eq!(app.settings.docked_native_plugin, None);
    }
}
