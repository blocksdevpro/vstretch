//! The tray's state: settings, the latest display reading, and automatic switching.

use anyhow::{Context, Result};
use tray_icon::{TrayIcon, TrayIconBuilder};

use super::{
    APP_LABEL,
    hotkey::Hotkey,
    menu::{Command, TrayMenu},
    theme, tray_icon,
};
use crate::{
    autostretch::{Action, AutoStretch, GameActivity},
    config::Config,
    display::{self, DisplayState, ModeKind},
    game, platform, startup,
    wake::{PowerWatcher, WakePlan, WakeRestore},
};

pub(super) struct App {
    pub(super) config: Config,
    pub(super) menu: TrayMenu,
    pub(super) icon: TrayIcon,
    pub(super) auto: AutoStretch,
    pub(super) display: DisplayState,
    pub(super) activity: GameActivity,
    pub(super) error: Option<String>,
    pub(super) hotkey: Hotkey,
    pub(super) hotkey_error: Option<String>,
    pub(super) power: PowerWatcher,
    pub(super) wake: WakeRestore,
}

impl App {
    pub(super) fn new() -> Result<Self> {
        let config = Config::load_or_init()?;
        let menu = TrayMenu::new(&config)?;
        let icon = TrayIconBuilder::new()
            .with_icon(tray_icon()?)
            .with_tooltip(format!("{APP_LABEL} | Right-click for display modes"))
            .with_menu(Box::new(menu.root.clone()))
            .with_menu_on_left_click(true)
            .build()
            .context("could not create the system tray icon")?;
        theme::follow_system_theme(&icon)?;
        let mut hotkey = Hotkey::new();
        let hotkey_error = hotkey.sync(&config);
        let mut app = Self {
            config,
            menu,
            icon,
            auto: AutoStretch::default(),
            display: DisplayState::default(),
            activity: GameActivity::Stopped,
            error: None,
            hotkey,
            hotkey_error,
            power: PowerWatcher::new(),
            wake: WakeRestore::default(),
        };
        app.refresh_display();
        if let Err(error) = startup::initialize(&mut app.config) {
            app.error = Some(format!("Startup settings: {error:#}"));
        }
        app.refresh_startup();
        app.sync_hotkey();
        app.sync_menu();
        Ok(app)
    }

    pub(super) fn sync_hotkey(&mut self) {
        self.hotkey_error = self.hotkey.sync(&self.config);
    }

    pub(super) fn refresh_display(&mut self) {
        self.display = DisplayState::read(self.config.native.as_ref());
    }

    pub(super) fn refresh_startup(&mut self) {
        match startup::enabled() {
            Ok(enabled) => {
                self.menu.startup.set_enabled(true);
                self.menu.startup.set_checked(enabled);
            }
            Err(error) => {
                self.menu.startup.set_checked(false);
                self.menu.startup.set_enabled(false);
                self.error = Some(format!("Startup settings: {error:#}"));
            }
        }
    }

    pub(super) fn sync_menu(&mut self) {
        self.menu.sync(&self.config, &self.display);
        let status = {
            let mut problems = Vec::new();
            if let Some(error) = &self.error {
                problems.push(error.clone());
            }
            if let Some(error) = &self.hotkey_error {
                problems.push(error.clone());
            }
            if problems.is_empty() {
                self.display
                    .current
                    .map(|mode| {
                        format!(
                            "Current display\t{} × {} @ {} Hz",
                            mode.width, mode.height, mode.refresh
                        )
                    })
                    .unwrap_or_else(|| "Current display unavailable".into())
            } else {
                problems.join(" | ")
            }
        };
        self.menu.status.set_text(
            status
                .chars()
                .take(110)
                .collect::<String>()
                .replace('&', "&&"),
        );
        let _ = self.icon.set_tooltip(Some(format!(
            "{APP_LABEL} | {}",
            status
                .chars()
                .take(100)
                .collect::<String>()
                .replace('\t', ": ")
        )));
    }

    pub(super) fn tick(&mut self) {
        // --tui may have saved a preset or native override while the tray runs.
        // Invalid external edits leave the last valid config in use and visible.
        match Config::load() {
            Ok(config) => self.config = config,
            Err(error) => self.error = Some(format!("Config: {error:#}")),
        }
        self.refresh_display();
        let activity = game::game_activity();
        if let Some(activity) = activity {
            self.activity = activity;
        }
        // While a wake restore is pending the display is mid-reset, which must
        // not look like an outside change to auto-stretch.
        if !self.restore_after_wake() && (activity.is_some() || !self.config.auto_stretch) {
            self.run_auto();
        }
        self.refresh_startup();
        self.sync_hotkey();
        self.sync_menu();
    }

    /// Windows can reset the display while sleeping. Put stretch back, keeping
    /// any restore target the automatic session already owns. Returns true while
    /// the wake is still being handled.
    fn restore_after_wake(&mut self) -> bool {
        let events = self.power.poll();
        let stretched = self.display.kind(&self.config.stretch) == ModeKind::Stretch;
        let plan = self
            .wake
            .observe(events, stretched, self.display.current.is_some());
        if plan == WakePlan::Idle {
            return false;
        }
        if !self
            .auto
            .allows_stretch_after_wake(self.config.auto_policy(), self.activity)
        {
            self.wake.cancel();
            return false;
        }
        if plan == WakePlan::Reapply {
            match self.display.apply_stretch(&self.config.stretch) {
                Ok(applied) => {
                    self.auto.reapplied(applied);
                    self.error = None;
                    self.refresh_display();
                }
                Err(error) => {
                    self.error = Some(format!("Restoring stretch after wake: {error:#}"));
                }
            }
        }
        true
    }

    pub(super) fn run_auto(&mut self) {
        let Some(current) = self.display.current else {
            return;
        };
        let policy = self.config.auto_policy();
        let Some(action) = self.auto.observe(policy, self.activity, current) else {
            return;
        };
        let result = match action {
            Action::Stretch { desktop } => {
                if self.display.native.is_none() {
                    self.error = Some("Auto-stretch paused: native resolution unavailable".into());
                    return;
                }
                self.display
                    .apply_stretch(&self.config.stretch)
                    .map(|applied| self.auto.applied(desktop, applied))
            }
            Action::Restore { desktop } => {
                display::change_mode(desktop).map(|()| self.auto.restored(policy, self.activity))
            }
        };
        match result {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(format!("Auto-stretch: {error:#}")),
        }
        self.refresh_display();
    }

    /// Runs a menu or hotkey command, reports any failure, and refreshes the
    /// menu. Returns true when the command asks the tray to exit.
    pub(super) fn run_command(&mut self, command: Command) -> bool {
        match self.handle(command) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                platform::show_error(&format!("{error:#}"));
            }
        }
        self.refresh_display();
        self.refresh_startup();
        self.sync_hotkey();
        self.sync_menu();
        false
    }

    pub(super) fn handle(&mut self, command: Command) -> Result<bool> {
        self.error = None;
        // An explicit display choice outranks anything remembered from before sleep.
        if matches!(
            command,
            Command::Native | Command::Stretch | Command::Toggle | Command::Preset(_)
        ) {
            self.wake.cancel();
        }
        self.refresh_display();
        match command {
            Command::Native => {
                self.display.restore_native(&self.config.stretch)?;
                self.auto.manual_choice();
            }
            Command::Stretch => {
                // Automatic sessions and manual stretch both need a known restore target.
                self.display.native()?;
                self.display.apply_stretch(&self.config.stretch)?;
                self.auto.manual_choice();
            }
            Command::Toggle => {
                self.display.toggle(&self.config.stretch)?;
                self.auto.manual_choice();
            }
            Command::Preset(profile) => {
                let was_stretch = self.display.kind(&self.config.stretch) == ModeKind::Stretch;
                self.config.update_and_save(|next| next.stretch = profile)?;
                if was_stretch {
                    self.display.apply_stretch(&self.config.stretch)?;
                    self.auto.manual_choice();
                }
            }
            Command::AutoStretch => {
                self.config
                    .update_and_save(|next| next.auto_stretch = !next.auto_stretch)?;
                if self.config.auto_stretch
                    && matches!(self.auto, AutoStretch::PausedUntilSessionEnds)
                {
                    self.auto = AutoStretch::Ready;
                }
                self.run_auto();
            }
            Command::RestoreOnAltTab => {
                self.config
                    .update_and_save(|next| next.restore_on_alt_tab = !next.restore_on_alt_tab)?;
                self.run_auto();
            }
            Command::Startup => {
                let enabled = !startup::enabled()?;
                self.config
                    .update_and_save(|next| next.start_with_windows = Some(enabled))?;
                startup::set_enabled(enabled)?;
            }
            Command::HotkeyEnabled => {
                let enabled = !self.config.hotkey_enabled;
                self.config.set_hotkey_enabled(enabled)?;
            }
            Command::Exit => {
                self.restore_on_exit()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn restore_on_exit(&mut self) -> Result<()> {
        if let Some(desktop) = display::current_mode()
            .ok()
            .and_then(|current| self.auto.restore_on_exit(current))
        {
            display::change_mode(desktop)
                .context("could not restore the desktop; select Native before exiting")?;
            self.auto = AutoStretch::Ready;
        }
        Ok(())
    }
}
