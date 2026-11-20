use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tokio_util::sync::CancellationToken;

use crate::engine::{Engine, FlowTable};
use crate::net::link::LinkSet;
use crate::net::pick::Picker;
use crate::prefs::{self, Prefs};
use crate::update::{self, Stage, UpdateState};
use crate::usage::Usage;
use crate::{autostart, commands, journal, popup, snapshot, tray, win};

const STOP_LIMIT: Duration = Duration::from_secs(5);
/// An engine that stops by itself is started again, unless it keeps doing so.
const RESTARTS_ALLOWED: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(600);
const REFRESH_EVERY: u64 = 3;
const SAVE_USAGE_EVERY: u64 = 30;

type FlowCounts = (Option<Instant>, HashMap<u64, (u64, u64)>);

pub struct Core {
    pub links: Arc<LinkSet>,
    pub picker: Arc<Picker>,
    prefs: Mutex<Prefs>,
    prefs_path: PathBuf,
    pub usage: Mutex<Usage>,
    usage_path: PathBuf,
    wintun: PathBuf,
    engine: tokio::sync::Mutex<Option<Engine>>,
    flows: RwLock<Option<Arc<FlowTable>>>,
    pub busy: AtomicBool,
    pub error: Mutex<Option<String>>,
    pub update: Mutex<UpdateState>,
    pub flow_seen: Mutex<FlowCounts>,
    restarts: Mutex<Vec<Instant>>,
    carried: Mutex<HashMap<String, u64>>,
    total: AtomicU64,
}

impl Core {
    fn new(dir: PathBuf, wintun: PathBuf) -> Core {
        let prefs_path = dir.join("settings.json");
        let usage_path = dir.join("usage.json");
        let prefs: Prefs = prefs::read(&prefs_path);
        let links = Arc::new(LinkSet::default());
        links.refresh();
        let picker = Arc::new(Picker::new(links.clone(), prefs.policy()));
        Core {
            links,
            picker,
            prefs: Mutex::new(prefs),
            prefs_path,
            usage: Mutex::new(prefs::read(&usage_path)),
            usage_path,
            wintun,
            engine: tokio::sync::Mutex::new(None),
            flows: RwLock::new(None),
            busy: AtomicBool::new(false),
            error: Mutex::new(None),
            update: Mutex::new(UpdateState::default()),
            flow_seen: Mutex::new((None, HashMap::new())),
            restarts: Mutex::new(Vec::new()),
            carried: Mutex::new(HashMap::new()),
            total: AtomicU64::new(0),
        }
    }

    pub fn prefs(&self) -> Prefs {
        self.prefs.lock().unwrap().clone()
    }

    /// Changes the settings, saves them and hands the picker its new policy.
    pub fn change(&self, edit: impl FnOnce(&mut Prefs)) {
        let mut prefs = self.prefs.lock().unwrap();
        edit(&mut prefs);
        prefs::write(&self.prefs_path, &*prefs);
        self.picker.set_policy(prefs.policy());
    }

    pub fn save_usage(&self) {
        prefs::write(&self.usage_path, &*self.usage.lock().unwrap());
    }

    pub fn flows(&self) -> Option<Arc<FlowTable>> {
        self.flows.read().unwrap().clone()
    }

    pub fn is_on(&self) -> bool {
        self.flows.read().unwrap().is_some()
    }

    async fn start(&self) -> Result<(), String> {
        let mut engine = self.engine.lock().await;
        if engine.is_some() {
            return Ok(());
        }
        self.busy.store(true, Relaxed);
        let dns = self.prefs().dns;
        let started = Engine::start(self.wintun.clone(), dns, self.picker.clone(), self.links.clone()).await;
        self.busy.store(false, Relaxed);
        match started {
            Ok(started) => {
                *self.flows.write().unwrap() = Some(started.flows.clone());
                *engine = Some(started);
                *self.error.lock().unwrap() = None;
                Ok(())
            }
            Err(err) => {
                let message = format!("Couldn't turn on: {err}");
                log::error!("{message}");
                *self.error.lock().unwrap() = Some(message.clone());
                Err(message)
            }
        }
    }

    async fn stop(&self) {
        let mut engine = self.engine.lock().await;
        let Some(running) = engine.take() else { return };
        self.busy.store(true, Relaxed);
        *self.flows.write().unwrap() = None;
        if tokio::time::timeout(STOP_LIMIT, running.stop()).await.is_err() {
            log::warn!("turning off took too long");
        }
        self.busy.store(false, Relaxed);
        *self.flow_seen.lock().unwrap() = (None, HashMap::new());
    }

    /// Turns on or off and remembers it for the next start.
    pub async fn set_power(&self, on: bool) -> Result<(), String> {
        let result = if on {
            self.start().await
        } else {
            self.stop().await;
            *self.error.lock().unwrap() = None;
            Ok(())
        };
        if result.is_ok() {
            self.change(|p| p.on = on);
        }
        result
    }

    /// Applies settings the adapter only takes as it is created.
    pub async fn restart(&self) -> Result<(), String> {
        if !self.is_on() {
            return Ok(());
        }
        self.stop().await;
        self.start().await
    }

    /// Once a second: rates, today's data, data caps, busy flows, a fresh
    /// network list now and then, and recovery when the engine stopped.
    async fn tick(&self, n: u64) {
        self.links.tick();
        let links = self.links.all();
        let busy = self.flows().map(|f| f.busy_by_link()).unwrap_or_default();
        for link in &links {
            link.set_busy(busy.get(&link.id).copied().unwrap_or(0));
        }

        let today = win::local_date();
        let prefs = self.prefs();
        {
            let mut usage = self.usage.lock().unwrap();
            let mut carried = self.carried.lock().unwrap();
            let mut sum = 0;
            for link in &links {
                let now = link.carried();
                sum += now;
                let before = carried.insert(link.id.clone(), now).unwrap_or(now);
                let p = prefs.link(&link.id, link.is_virtual());
                let used = usage.count_link(&link.id, p.period_of(today), now.saturating_sub(before));
                link.set_capped(p.cap_bytes().is_some_and(|cap| used >= cap));
            }
            let before = self.total.swap(sum, Relaxed);
            usage.count_today(today, sum.saturating_sub(before));
        }
        if n.is_multiple_of(SAVE_USAGE_EVERY) {
            self.save_usage();
        }
        if n.is_multiple_of(REFRESH_EVERY) {
            let links = self.links.clone();
            let _ = tokio::task::spawn_blocking(move || links.refresh()).await;
        }
        self.recover().await;
    }

    async fn recover(&self) {
        let stopped = match self.engine.try_lock() {
            Ok(engine) => engine.as_ref().is_some_and(|e| !e.running()),
            Err(_) => false,
        };
        if !stopped {
            return;
        }
        log::error!("the engine stopped by itself");
        self.stop().await;
        let again = {
            let mut restarts = self.restarts.lock().unwrap();
            restarts.retain(|at| at.elapsed() < RESTART_WINDOW);
            restarts.push(Instant::now());
            restarts.len() <= RESTARTS_ALLOWED
        };
        if !again {
            *self.error.lock().unwrap() = Some("NetBraid keeps stopping. Turn it on again to retry.".into());
        } else if self.start().await.is_ok() {
            log::warn!("turned back on after stopping unexpectedly");
        }
    }
}

/// Sends the current state to the tray, and to the popup while it is open.
pub fn publish(app: &AppHandle, core: &Core) {
    let snapshot = snapshot::build(core);
    tray::reflect(app, snapshot.on, snapshot::tooltip(&snapshot));
    if popup::visible(app) {
        let _ = app.emit_to(popup::LABEL, "snapshot", &snapshot);
    }
}

pub async fn check_for_update(core: &Core) {
    {
        let mut state = core.update.lock().unwrap();
        if matches!(state.stage, Stage::Checking | Stage::Downloading) {
            return;
        }
        state.stage = Stage::Checking;
        state.error = None;
    }
    let found = tokio::task::spawn_blocking(update::latest)
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    let mut state = core.update.lock().unwrap();
    match found {
        Ok(Some(release)) if update::is_newer(&release.version, update::VERSION) => {
            log::info!("NetBraid {} is available", release.version);
            state.stage = Stage::Available;
            state.release = Some(release);
        }
        Ok(_) => {
            state.stage = Stage::Current;
            state.release = None;
        }
        Err(err) => {
            log::warn!("update check failed: {err}");
            state.stage = Stage::Failed;
            state.error = Some(err);
        }
    }
}

/// Downloads the installer, checks it, starts it and quits, so the
/// installer can replace this copy.
pub async fn install_update(app: &AppHandle, core: &Arc<Core>) -> Result<(), String> {
    let release = {
        let mut state = core.update.lock().unwrap();
        let Some(release) = state.release.clone() else {
            return Err("Check for updates first.".into());
        };
        state.stage = Stage::Downloading;
        state.progress = Some(0);
        state.error = None;
        release
    };
    let progress = core.clone();
    let downloaded = tokio::task::spawn_blocking(move || {
        update::download(&release, |p| progress.update.lock().unwrap().progress = Some(p))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    let installer = match downloaded {
        Ok(path) => path,
        Err(err) => {
            let mut state = core.update.lock().unwrap();
            state.stage = Stage::Failed;
            state.error = Some(err.clone());
            return Err(err);
        }
    };
    // Off without remembering it, so the new version turns on again by itself.
    core.stop().await;
    core.save_usage();
    std::process::Command::new(&installer)
        .spawn()
        .map_err(|e| format!("Couldn't start the installer: {e}"))?;
    log::info!("installing {}", installer.display());
    app.exit(0);
    Ok(())
}

async fn every_second(app: AppHandle, core: Arc<Core>) {
    let mut every = tokio::time::interval(Duration::from_secs(1));
    let mut n = 0u64;
    loop {
        every.tick().await;
        n += 1;
        core.tick(n).await;
        publish(&app, &core);
    }
}

fn wintun_path(app: &AppHandle) -> PathBuf {
    app.path()
        .resolve("resources/wintun.dll", BaseDirectory::Resource)
        .unwrap_or_else(|_| PathBuf::from("resources/wintun.dll"))
}

pub fn run() {
    let quiet_start = std::env::args().any(|a| a == autostart::ARG);
    let stop_probes = CancellationToken::new();
    let probes = stop_probes.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| popup::show(app)))
        .manage(popup::PopupState::default())
        .invoke_handler(tauri::generate_handler![
            popup::popup_fit,
            popup::popup_hide,
            commands::snapshot,
            commands::power,
            commands::link_update,
            commands::link_cap,
            commands::usage_reset,
            commands::rules_save,
            commands::options_save,
            commands::flows_list,
            commands::log_lines,
            commands::update_check,
            commands::update_install,
            commands::open_link,
            commands::quit,
        ])
        .setup(move |app| {
            let dir = app.path().app_config_dir()?;
            journal::start(&dir.join("logs"));
            let core = Arc::new(Core::new(dir, wintun_path(app.handle())));
            let prefs = core.prefs();
            app.manage(core.clone());
            tray::create(app.handle())?;

            tauri::async_runtime::spawn(crate::net::probe::run(core.links.clone(), probes));
            tauri::async_runtime::spawn(every_second(app.handle().clone(), core.clone()));
            let startup = core.clone();
            tauri::async_runtime::spawn(async move {
                if prefs.autostart && !autostart::exists() {
                    let restored = tauri::async_runtime::spawn_blocking(autostart::enable)
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|done| done.map_err(|e| e.to_string()));
                    if let Err(err) = restored {
                        log::warn!("could not restore starting with Windows: {err}");
                    }
                }
                if prefs.on {
                    let _ = startup.set_power(true).await;
                }
                if prefs.check_updates {
                    check_for_update(&startup).await;
                }
            });
            if !quiet_start {
                popup::show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != popup::LABEL {
                return;
            }
            match event {
                WindowEvent::Focused(false) => popup::hide(window.app_handle()),
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    popup::hide(window.app_handle());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("NetBraid could not start")
        .run(move |app, event| {
            if let RunEvent::Exit = event {
                stop_probes.cancel();
                if let Some(core) = app.try_state::<Arc<Core>>() {
                    let core = core.inner().clone();
                    tauri::async_runtime::block_on(async move {
                        core.stop().await;
                        core.save_usage();
                    });
                }
            }
        });
}
