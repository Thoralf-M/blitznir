//! The tray daemon on Freya: no window until the panel opens, the tray, and
//! the threads around one [`Dictator`] and one [`Panel`] (model, hotkeys,
//! the permission watch), all from the shared half.

use crate::panel::{Hooks, Mode, Panel, Parts};
use crate::tray::Tray;
use crate::zap::caret::Caret;
use blitznir::audio::Source;
use blitznir::session::{Dictator, Note};
use blitznir::{logging, platform, stt};
use blitznir_core::config::{Config, Paths};
use blitznir_core::dictation::Phase;
use blitznir_core::keys::{Action, Detector};
use blitznir_core::platform::Platform;
use blitznir_core::store::Store;
use blitznir_core::tasks::Kind;
use blitznir_core::{Error, Result};
use freya::prelude::{LaunchConfig, LaunchProxy, NativeEvent, launch};
use freya::winit::event_loop::EventLoop;
use std::fs::{File, TryLockError};
use std::sync::{Arc, OnceLock, Weak, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// How often missing permissions are looked at again.
const PERMISSION_POLL: Duration = Duration::from_secs(3);
/// Opens the panel at start with this text as the selection: the tests' way
/// in, as the flock single instance takes no arguments from a second start.
const OPEN_PANEL: &str = "BLITZNIR_OPEN_PANEL";

/// One daemon per data dir: an flock on `<data>/.lock`, held while the
/// returned file lives. None: another one holds it.
fn single_instance(paths: &Paths) -> Result<Option<File>> {
    std::fs::create_dir_all(&paths.dir)?;
    let file = File::create(paths.dir.join(".lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Accessory from launch on macOS: no Dock icon, no app menu. And
/// not activated at launch, winit's default: a daemon starting at login must
/// leave the app in front where it is, the panel's paste target.
fn event_loop() -> Result<EventLoop<NativeEvent>> {
    #[allow(unused_mut)]
    let mut builder = EventLoop::<NativeEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use freya::winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
        builder.with_activate_ignoring_other_apps(false);
    }
    builder
        .build()
        .map_err(|e| Error::Platform(format!("winit: {e}")))
}

/// Markdown and the Settings editor ask for "monospace", which macOS does not
/// resolve; fontconfig does on Linux.
fn fonts(config: LaunchConfig) -> LaunchConfig {
    match std::fs::read("/System/Library/Fonts/Menlo.ttc") {
        Ok(menlo) if cfg!(target_os = "macos") => config.with_font("monospace", menlo),
        _ => config,
    }
}

/// The lock, then the log: only the start that holds the lock opens
/// blitznir.log (emptied there once too big); a second start would empty
/// the running one's. None: another one holds it.
fn start(paths: &Paths) -> Result<Option<File>> {
    let lock = single_instance(paths);
    let held = matches!(lock, Ok(Some(_)));
    logging::init(held.then(|| paths.log()).as_deref());
    lock
}

pub fn run() -> Result<()> {
    let paths = Paths::user()?;
    let Some(_lock) = start(&paths)? else {
        log::info!("already running; a second start was ignored");
        return Ok(());
    };
    log::info!(
        "Blitznir {} starting; data in {}",
        env!("CARGO_PKG_VERSION"),
        paths.dir.display()
    );
    let (cfg, broken_config) = match Config::load(&paths) {
        Ok(cfg) => (cfg, false),
        Err(e) => {
            log::error!("{e}; running on the built-in defaults");
            (Config::default(), true)
        }
    };
    let store = Arc::new(Store::new(paths.clone(), history_cap(&cfg, broken_config))?);
    // after the lock: a second start must not take the running one's recording
    if let Err(e) = store.recover_recording() {
        log::error!("cannot recover recording.txt: {e}");
    }
    let event_loop = event_loop()?;
    let proxy = LaunchProxy(event_loop.create_proxy());
    let platform = shell_platform(platform::current(), &proxy);
    let tray = Tray::new(platform.clone(), proxy.clone());
    let caret = Caret::new(platform.clone(), proxy.clone(), cfg.panel.dictation_effect);
    let own = Arc::new(OnceLock::new());
    let dictator = dictator(&platform, &store, &cfg, &tray, &caret, &own);
    if broken_config {
        tray.flash("config.toml is broken: see blitznir.log");
    }
    let (reload, reloads) = mpsc::channel();
    let parts = Parts {
        platform: platform.clone(),
        store,
        paths: paths.clone(),
        cfg: cfg.clone(),
        tray: tray.clone(),
        proxy,
    };
    let panel = Panel::new(parts, hooks(&tray, &dictator, &caret, reload))?;
    let _ = own.set(Arc::downgrade(&panel));
    tray.set_panel(&panel);
    hotkeys(&platform, &cfg, dictator.clone(), panel.clone(), reloads);
    load_model(&paths, &cfg, dictator, tray.clone())?;
    let t = tray.clone();
    thread::Builder::new()
        .name("permissions".into())
        .spawn(move || watch_permissions(&*platform, &t))?;
    let start = std::env::var(OPEN_PANEL).ok();
    let make = tray.clone();
    let config = LaunchConfig::new()
        .with_event_loop(event_loop)
        .with_exit_on_close(false)
        .with_tray(
            move || make.build(),
            move |ev, mut ctx| tray.on_event(ev, &mut ctx),
        )
        // runs once the loop is up, when a window can be made
        .with_task(move |_| async move {
            if let Some(text) = start {
                panel.open(Mode::Rewrite, Some(text));
            }
        });
    #[cfg(target_os = "macos")]
    platform::no_title_bar_zoom(); // the header lives in the title bar (panel/window.rs)
    launch(fonts(config));
    log::info!("quit");
    Ok(())
}

/// The dictation, its phases shown in the tray and by the caret's wave, and
/// the panel's (`own`, set once the panel exists: it is made after the
/// dictator) for a dictation into its composer.
fn dictator(
    platform: &Arc<dyn Platform>,
    store: &Arc<Store>,
    cfg: &Config,
    tray: &Arc<Tray>,
    caret: &Arc<Caret>,
    own: &Arc<OnceLock<Weak<Panel>>>,
) -> Arc<Dictator> {
    let (t, c, own) = (tray.clone(), caret.clone(), own.clone());
    Dictator::new(
        platform.clone(),
        store.clone(),
        cfg.clone(),
        Source::Mic,
        move |note| match note {
            Note::Phase(phase) => {
                t.set_phase(phase);
                c.phase(phase);
                if let Some(panel) = own.get().and_then(Weak::upgrade) {
                    panel.set_dictating(matches!(phase, Phase::Live | Phase::Batch));
                }
            }
            Note::Flash(msg) => t.flash(msg),
            Note::Dictated(text, shown) => t.dictated(text, shown),
        },
    )
}

/// Linux: the selection on winit's Wayland connection, not GTK's.
#[cfg(target_os = "linux")]
fn shell_platform(shared: Arc<dyn Platform>, proxy: &LaunchProxy) -> Arc<dyn Platform> {
    crate::wayland::Wayland::new(shared, proxy.clone())
}

#[cfg(target_os = "macos")]
fn shell_platform(shared: Arc<dyn Platform>, _proxy: &LaunchProxy) -> Arc<dyn Platform> {
    shared
}

/// The panel shows a job in the tray; a saved config.toml reaches the
/// dictation and the hotkeys.
fn hooks(
    tray: &Arc<Tray>,
    dictator: &Arc<Dictator>,
    caret: &Arc<Caret>,
    reload: mpsc::Sender<Config>,
) -> Hooks {
    let (t, d) = (tray.clone(), dictator.clone());
    let busy = move |title: Option<&'static str>| match title {
        Some(title) => t.show(title),
        None => t.set_phase(d.phase()),
    };
    let (d, c) = (dictator.clone(), caret.clone());
    Hooks {
        busy: Box::new(busy),
        config: Box::new(move |cfg: &Config| {
            d.set_config(cfg.clone());
            c.set_on(cfg.panel.dictation_effect);
            let _ = reload.send(cfg.clone()); // fails only when hotkeys never started
        }),
    }
}

/// Download if needed, load, hand the model to the dictator.
fn load_model(paths: &Paths, cfg: &Config, dictator: Arc<Dictator>, tray: Arc<Tray>) -> Result<()> {
    let model = cfg.dictation.model;
    let dir = stt::model_dir(paths, model);
    thread::Builder::new().name("model".into()).spawn(move || {
        let loaded = stt::ensure_model(&dir, model, &mut |pct| {
            tray.show(&format!("Downloading model {pct}%"));
        })
        .and_then(|()| {
            tray.set_phase(Phase::Loading);
            stt::Parakeet::load(&dir)
        });
        match loaded {
            Ok(rec) => dictator.ready(Box::new(rec)),
            Err(e) => {
                log::error!("speech model: {e}");
                tray.show("No speech model: see blitznir.log");
            }
        }
    })?;
    Ok(())
}

fn detector(cfg: &Config) -> Detector {
    let tap = Duration::from_millis(cfg.hotkeys.double_tap_ms);
    Detector::new(cfg.bindings(), tap)
}

/// Hotkey events → the detector → an action. A saved config.toml arrives on
/// `reloads` and takes effect with the next key event.
fn hotkeys(
    platform: &Arc<dyn Platform>,
    cfg: &Config,
    dictator: Arc<Dictator>,
    panel: Arc<Panel>,
    reloads: mpsc::Receiver<Config>,
) {
    let (tx, rx) = mpsc::channel();
    if let Err(e) = platform.listen(tx) {
        log::error!("no hotkeys: {e}");
        return;
    }
    let mut detector = detector(cfg);
    let run = move || {
        for ev in rx {
            if let Some(cfg) = reloads.try_iter().last() {
                detector = self::detector(&cfg);
                log::info!("hotkeys reloaded");
            }
            panel.saw(ev);
            match detector.feed(ev, Instant::now()) {
                Some(Action::Live) => dictator.press(true),
                Some(Action::Batch) => dictator.press(false),
                Some(Action::Rewrite) => panel.open(Mode::Rewrite, None),
                Some(Action::Chat) => panel.open(Mode::Chat, None),
                Some(Action::Quick) => panel.quick(Kind::Rewrite, None),
                Some(Action::Translate) => panel.quick(Kind::Translate, None),
                Some(Action::Button(i)) => panel.quick(Kind::Rewrite, Some(i)),
                Some(Action::Undo) => panel.undo(),
                None => {}
            }
        }
    };
    if let Err(e) = thread::Builder::new().name("hotkeys".into()).spawn(run) {
        log::error!("no hotkeys: {e}");
    }
}

/// The tray menu offers each missing grant; rebuild it when one changes.
fn watch_permissions(platform: &dyn Platform, tray: &Tray) {
    let mut last = platform.missing_permissions();
    if !last.is_empty() {
        log::warn!("missing permissions: {}", last.join(", "));
    }
    loop {
        thread::sleep(PERMISSION_POLL);
        let now = platform.missing_permissions();
        if now != last {
            log::info!("missing permissions now: {now:?}");
            tray.refresh();
            last = now;
        }
    }
}

/// How many history rows of each kind the store keeps: config.toml's, or
/// all of them while it does not parse. The built-in 10 would trim the
/// history for good at the next write, though config.toml asked for more.
fn history_cap(cfg: &Config, broken_config: bool) -> usize {
    if broken_config {
        usize::MAX
    } else {
        cfg.dictation.history_max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_config_trims_no_history() {
        let dir = std::env::temp_dir().join(format!("blitznir-keep-{}", std::process::id()));
        let paths = Paths::new(&dir);
        let fine = Store::new(paths.clone(), 100).unwrap(); // config.toml's
        for i in 0..41 {
            fine.add_history(&format!("said {i}"), "live").unwrap();
        }
        drop(fine);
        let broken = Store::new(paths, history_cap(&Config::default(), true)).unwrap();
        broken.add_history("said once more", "live").unwrap();
        assert_eq!(
            broken.history().len(),
            42,
            "all kept while config.toml is broken"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_start_leaves_the_running_ones_log_alone() {
        let dir = std::env::temp_dir().join(format!("blitznir-second-{}", std::process::id()));
        let paths = Paths::new(&dir);
        let running = single_instance(&paths).unwrap().expect("the first start");
        let big = 6 << 20; // past the size a start empties the log at
        std::fs::write(paths.log(), vec![b'x'; big]).unwrap();
        assert!(start(&paths).unwrap().is_none(), "a second start");
        assert_eq!(std::fs::metadata(paths.log()).unwrap().len(), big as u64);
        drop(running);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
