//! The menu-bar item on Freya's tray (tray-icon + muda): the state as its
//! title on macOS, as its icon on Linux, and the menu.
//!
//! tray-icon is !Send and lives on one thread: the main thread on macOS,
//! Freya's own GTK thread on Linux (freya-winit lib.rs:189), and Freya hands
//! out no handle to it. So the icon sits in a thread-local there, and every
//! change is posted to that thread: `LaunchProxy::post_callback` on macOS,
//! `glib::idle_add_once` on Linux.

use crate::autostart;
use crate::panel::Panel;
use blitznir::platform;
use blitznir::session::{FLASH, title};
use blitznir_core::dictation::Phase;
use blitznir_core::lock;
use blitznir_core::platform::Platform;
#[cfg(target_os = "linux")]
use freya::prelude::LaunchConfig;
use freya::prelude::{LaunchProxy, RendererContext};
use freya::tray::menu::{self, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use freya::tray::{Icon, TrayEvent, TrayIcon, TrayIconBuilder};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// Lightning (zap.rs) makes the hammer glow up to this strength and back.
pub const GLOW_PEAK: usize = 5;
/// A question to the tray's thread gives up after this.
const ASK_WAIT: Duration = Duration::from_millis(200);

thread_local! {
    /// Set on the tray's thread by [`Tray::build`]; None on every other.
    static ICON: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// hammer.png is white on black; a template image wants the shape in alpha.
#[cfg(target_os = "macos")]
fn state_icon(_phase: Phase) -> Icon {
    let png = image::load_from_memory(include_bytes!("../../../assets/hammer.png"))
        .expect("assets/hammer.png is a PNG")
        .to_rgba8();
    let (w, h) = png.dimensions();
    let rgba = png.pixels().flat_map(|p| [0, 0, 0, p[0]]).collect();
    Icon::from_rgba(rgba, w, h).expect("width × height × 4 bytes")
}

/// The idle icon, then the glow's strengths 1 to [`GLOW_PEAK`]
/// (assets/make-glow.sh), shaped in alpha already.
#[cfg(target_os = "macos")]
fn glow_frames() -> Vec<Icon> {
    let pngs: [&[u8]; GLOW_PEAK] = [
        include_bytes!("../../../assets/glow-1.png"),
        include_bytes!("../../../assets/glow-2.png"),
        include_bytes!("../../../assets/glow-3.png"),
        include_bytes!("../../../assets/glow-4.png"),
        include_bytes!("../../../assets/glow-5.png"),
    ];
    let glow = pngs.map(freya::prelude::LaunchConfig::tray_icon);
    std::iter::once(state_icon(Phase::Idle))
        .chain(glow)
        .collect()
}

/// None: the Linux tray shows no glow, only the state's icon.
#[cfg(target_os = "linux")]
fn glow_frames() -> Vec<Icon> {
    vec![]
}

/// A Linux tray has no title text, so the icon shows the state.
#[cfg(target_os = "linux")]
fn state_icon(phase: Phase) -> Icon {
    let png: &[u8] = match phase {
        Phase::Idle => include_bytes!("../../../assets/tray-idle.png"),
        Phase::Live | Phase::Batch => include_bytes!("../../../assets/tray-recording.png"),
        Phase::Loading | Phase::Finishing => include_bytes!("../../../assets/tray-working.png"),
    };
    LaunchConfig::tray_icon(png)
}

fn tooltip(text: &str) -> &str {
    if text.is_empty() { "Blitznir" } else { text }
}

struct Shown {
    phase: Phase,
    /// The title (macOS) and tooltip: the phase's, or a message.
    text: String,
    /// A flash is on screen until then.
    flash_until: Option<Instant>,
}

/// What the menu shows, gathered off the tray's thread.
struct MenuData {
    missing: Vec<&'static str>,
    at_login: bool,
    /// There is a paste-back to take back.
    undo: bool,
}

pub struct Tray {
    platform: Arc<dyn Platform>,
    shown: Mutex<Shown>,
    /// Set once both exist: the panel answers in the tray, the tray opens it.
    panel: OnceLock<Weak<Panel>>,
    /// [`glow_frames`], decoded once.
    glow: Vec<Icon>,
    #[cfg(target_os = "macos")]
    proxy: LaunchProxy,
}

impl Tray {
    pub fn new(platform: Arc<dyn Platform>, proxy: LaunchProxy) -> Arc<Tray> {
        #[cfg(target_os = "linux")]
        drop(proxy); // glib reaches Freya's GTK thread by itself
        Arc::new(Tray {
            platform,
            shown: Mutex::new(Shown {
                phase: Phase::Loading,
                text: title(Phase::Loading).into(),
                flash_until: None,
            }),
            panel: OnceLock::new(),
            glow: glow_frames(),
            #[cfg(target_os = "macos")]
            proxy,
        })
    }

    pub fn set_panel(&self, panel: &Arc<Panel>) {
        let _ = self.panel.set(Arc::downgrade(panel));
        self.refresh();
    }

    fn panel(&self) -> Option<Arc<Panel>> {
        self.panel.get()?.upgrade()
    }

    /// Freya calls this once, on the tray's thread; it shows what the
    /// updates posted before it would have shown.
    pub fn build(&self) -> TrayIcon {
        let (phase, text) = {
            let shown = lock(&self.shown);
            (shown.phase, shown.text.clone())
        };
        // a template on macOS: drawn light or dark to suit the menu bar
        #[cfg(target_os = "macos")]
        let builder = TrayIconBuilder::new().with_icon_templated(state_icon(phase));
        #[cfg(target_os = "linux")]
        let builder = TrayIconBuilder::new().with_icon(state_icon(phase));
        let builder = builder
            .with_tooltip(tooltip(&text))
            .with_menu(Box::new(build_menu(&self.menu_data())));
        #[cfg(target_os = "macos")]
        let builder = builder.with_title(&text);
        let tray = builder
            .build()
            .expect("the status item: Freya takes no error here");
        ICON.set(Some(tray.clone()));
        log::info!("tray ready");
        tray
    }

    /// Run `f` on the tray's thread. Before [`Tray::build`] it does nothing.
    fn on_icon(&self, f: impl FnOnce(&TrayIcon) + Send + 'static) {
        let run = move || {
            ICON.with_borrow(|icon| {
                if let Some(icon) = icon {
                    f(icon);
                }
            });
        };
        #[cfg(target_os = "macos")]
        drop(self.proxy.post_callback(move |_| run()));
        #[cfg(target_os = "linux")]
        gtk::glib::idle_add_once(run);
    }

    /// Put `text` in the menu bar as is (download progress, errors that stay).
    pub fn show(&self, text: &str) {
        lock(&self.shown).text = text.into();
        let text = text.to_string();
        self.on_icon(move |icon| {
            #[cfg(target_os = "macos")]
            icon.set_title(Some(&text));
            let _ = icon.set_tooltip(Some(tooltip(&text)));
        });
    }

    pub fn set_phase(&self, phase: Phase) {
        let mut shown = lock(&self.shown);
        shown.phase = phase;
        let flashing = shown.flash_until.is_some();
        drop(shown);
        #[cfg(target_os = "linux")]
        self.on_icon(move |icon| drop(icon.set_icon(Some(state_icon(phase)))));
        if !flashing {
            self.show(title(phase));
        }
    }

    /// A message for [`FLASH`]; then the state's title comes back.
    pub fn flash(self: &Arc<Self>, msg: &'static str) {
        let until = Instant::now() + FLASH;
        lock(&self.shown).flash_until = Some(until);
        self.show(msg);
        let me = self.clone();
        thread::spawn(move || {
            thread::sleep(FLASH);
            let mut shown = lock(&me.shown);
            if shown.flash_until == Some(until) {
                shown.flash_until = None;
                let phase = shown.phase;
                drop(shown);
                me.show(title(phase));
            }
        });
    }

    /// Lightning: the hammer at glow `level`, 0 (the idle icon) to [`GLOW_PEAK`].
    pub fn glow(&self, level: usize) {
        if let Some(img) = self.glow.get(level).cloned() {
            // plain set_icon drops the template flag: black in a dark menu bar
            #[cfg(target_os = "macos")]
            self.on_icon(move |icon| drop(icon.set_icon_templated(Some(img))));
            #[cfg(target_os = "linux")]
            drop(img); // no frames there (glow_frames)
        }
    }

    /// The status item's top left and height in physical pixels of its
    /// screen's scale; None where the tray does not say (Linux). Never on the
    /// tray's thread: it waits for it.
    pub fn rect(&self) -> Option<((f64, f64), f64)> {
        if cfg!(target_os = "linux") {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.on_icon(move |icon| {
            let _ = tx.send(icon.rect());
        });
        let r = rx.recv_timeout(ASK_WAIT).ok().flatten()?;
        Some(((r.position.x, r.position.y), f64::from(r.size.height)))
    }

    /// A dictation ended: the menu's Undo, and its rewrite (the panel's,
    /// when `[dictation] rewrite` is set).
    pub fn dictated(&self, text: String, shown: bool) {
        self.refresh();
        if let Some(p) = self.panel() {
            p.rewrite_dictation(text, shown);
        }
    }

    /// Rebuild the menu: after a dictation, a permission change, a toggle.
    pub fn refresh(&self) {
        let data = self.menu_data();
        self.on_icon(move |icon| icon.set_menu(Some(Box::new(build_menu(&data)))));
    }

    fn menu_data(&self) -> MenuData {
        MenuData {
            missing: self.platform.missing_permissions(),
            at_login: autostart::enabled(),
            undo: self.panel().is_some_and(|p| p.can_undo()),
        }
    }

    /// A menu click, on the event loop's thread.
    pub fn on_event(self: &Arc<Self>, ev: TrayEvent, ctx: &mut RendererContext) {
        let TrayEvent::Menu(MenuEvent { id }) = ev else {
            return;
        };
        match id.0.as_str() {
            "quit" => ctx.exit(),
            // the app in front keeps focus while the menu is open: ⌘C reaches it
            "panel" | "settings" => {
                if let Some(p) = self.panel() {
                    p.open_from_tray(id.0 == "settings");
                }
            }
            "undo" => {
                if let Some(p) = self.panel() {
                    p.undo();
                }
            }
            "login" => {
                let on = !autostart::enabled();
                match autostart::set(on) {
                    Ok(()) => log::info!("start at login: {on}"),
                    Err(e) => log::error!("start at login: {e}"),
                }
                self.refresh();
            }
            id => {
                if let Some(url) = id.strip_prefix("perm:").and_then(platform::permission_help)
                    && let Err(e) = self.platform.open_url(url)
                {
                    log::error!("{url}: {e}");
                }
            }
        }
    }
}

/// On the tray's thread: muda builds its native menus there.
fn build_menu(d: &MenuData) -> Menu {
    let menu = Menu::new();
    if let Err(e) = fill(&menu, d) {
        log::error!("tray menu: {e}");
    }
    menu
}

fn fill(menu: &Menu, d: &MenuData) -> menu::Result<()> {
    let item = |id: String, text: &str| MenuItem::with_id(id, text, true, None);
    for name in &d.missing {
        // Linux has no pane to open: say what is missing, greyed out
        let pane = platform::permission_help(name).is_some();
        let text = if pane {
            format!("Grant {name}…")
        } else {
            format!("Missing: {name} (see the README)")
        };
        menu.append(&MenuItem::with_id(format!("perm:{name}"), text, pane, None))?;
    }
    if !d.missing.is_empty() {
        menu.append(&PredefinedMenuItem::separator())?;
    }
    // the window: rewrite, translate and chat about the selection, the chats
    menu.append(&item("panel".into(), "Open Blitznir"))?;
    let undo = MenuItem::with_id("undo", "Undo last replace", d.undo, None);
    menu.append(&undo)?;
    let login = CheckMenuItem::with_id("login", "Start at login", true, d.at_login, None);
    menu.append(&login)?;
    // the panel's Settings tab, where Save validates: never the file behind its back
    menu.append(&item("settings".into(), "Settings…"))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id("quit", "Quit Blitznir", true, None))
}
