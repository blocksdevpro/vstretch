//! TUI state and input handling: screens, selections, display actions, and updates.

use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use ratatui::widgets::ListState;

use crate::{
    config::{Config, presets::POPULAR_STRETCH},
    display::{self, DisplayState, Mode},
    update::{self, Release},
};

type UpdateResult<T> = Receiver<std::result::Result<T, String>>;

pub(super) enum UpdateState {
    Checking(UpdateResult<Option<Release>>),
    Current,
    Available(Release),
    Failed(String),
    Installing {
        version: String,
        result: UpdateResult<()>,
    },
    Installed(String),
}

/// Runs network and replacement work without blocking terminal input or drawing.
fn background<T: Send + 'static>(
    task: impl FnOnce() -> Result<T> + Send + 'static,
) -> UpdateResult<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(task().map_err(|error| format!("{error:#}")));
    });
    receiver
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    Home,
    Presets,
    Native,
}

#[derive(Clone, Copy)]
pub(super) enum HomeAction {
    Toggle,
    Stretch,
    Native,
    ChangeStretch,
    ChangeNative,
    Quit,
}

pub(super) const HOME_ACTIONS: &[(HomeAction, &str)] = &[
    (HomeAction::Toggle, "Toggle  native ↔ stretch"),
    (HomeAction::Stretch, "Apply stretch (default profile)"),
    (HomeAction::Native, "Apply native"),
    (HomeAction::ChangeStretch, "Change default stretch…"),
    (HomeAction::ChangeNative, "Change default native…"),
    (HomeAction::Quit, "Quit"),
];

pub(super) struct App {
    pub(super) screen: Screen,
    pub(super) home_state: ListState,
    pub(super) preset_state: ListState,
    pub(super) native_state: ListState,
    pub(super) native_modes: Vec<Mode>,
    pub(super) config: Config,
    pub(super) display: DisplayState,
    pub(super) status: String,
    pub(super) status_ok: bool,
    pub(super) should_quit: bool,
    pub(super) update: UpdateState,
    pub(super) confirm_update: bool,
}

impl App {
    pub(super) fn new() -> Result<Self> {
        let config = Config::load_or_init()?;
        let mut home_state = ListState::default();
        home_state.select(Some(0));
        let mut preset_state = ListState::default();
        let default_idx = POPULAR_STRETCH
            .iter()
            .position(|x| x.width == config.stretch.width && x.height == config.stretch.height)
            .or_else(|| {
                POPULAR_STRETCH
                    .iter()
                    .position(|x| x.width == 1440 && x.height == 1080)
            })
            .unwrap_or(0);
        preset_state.select(Some(default_idx));

        let mut native_state = ListState::default();
        native_state.select(Some(0));

        let mut app = Self {
            screen: Screen::Home,
            home_state,
            preset_state,
            native_state,
            native_modes: Vec::new(),
            config,
            display: DisplayState::default(),
            status: "Ready — pick an action".into(),
            status_ok: true,
            should_quit: false,
            update: UpdateState::Checking(background(update::check)),
            confirm_update: false,
        };
        app.refresh_display();
        Ok(app)
    }

    fn refresh_display(&mut self) {
        self.display = DisplayState::read(self.config.native.as_ref());
    }

    fn set_ok(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_ok = true;
    }

    fn set_err(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_ok = false;
    }

    pub(super) fn stretch_label(&self) -> String {
        self.config.stretch.name()
    }

    fn effective_native(&self) -> Result<Mode> {
        self.display
            .native
            .context("could not resolve native resolution")
    }

    fn run_toggle(&mut self) {
        if let Err(e) = self.effective_native() {
            self.set_err(format!("Could not read native: {e}"));
            return;
        }
        let name = self.config.stretch.name();
        match self.display.toggle(&self.config.stretch) {
            Ok(mode) => {
                self.set_ok(format!("Toggled → {}  [{name}]", mode.label()));
                self.refresh_display();
            }
            Err(e) => self.set_err(format!("Toggle failed: {e}")),
        }
    }

    fn run_stretch(&mut self) {
        let name = self.config.stretch.name();
        match self.display.apply_stretch(&self.config.stretch) {
            Ok(mode) => {
                self.set_ok(format!("Stretch → {}  [{name}]", mode.label()));
                self.refresh_display();
            }
            Err(e) => self.set_err(format!("Stretch failed: {e}")),
        }
    }

    fn run_native(&mut self) {
        if let Err(e) = self.effective_native() {
            self.set_err(format!("Could not read native: {e}"));
            return;
        }
        match self.display.restore_native(&self.config.stretch) {
            Ok(native) => {
                self.set_ok(format!("Native → {}", native.label()));
                self.refresh_display();
            }
            Err(e) => self.set_err(format!("Native failed: {e}")),
        }
    }

    pub(super) fn native_choice_len(&self) -> usize {
        self.native_modes.len() + 1
    }

    fn open_native_picker(&mut self) {
        self.native_modes = display::available_modes();
        if let Some(over) = &self.config.native
            && display::preferred_override_mode(&self.native_modes, over, self.display.panel)
                .is_none()
        {
            self.native_modes
                .insert(0, display::native_override_mode(over, self.display.panel));
        }
        let idx = match &self.config.native {
            None => 0,
            Some(over) => {
                display::preferred_override_mode(&self.native_modes, over, self.display.panel)
                    .and_then(|pref| self.native_modes.iter().position(|m| *m == pref))
                    .map(|i| i + 1)
                    .unwrap_or(0)
            }
        };
        self.native_state.select(Some(idx));
        self.screen = Screen::Native;
        self.set_ok("Pick a native resolution, or Auto for panel detect");
    }

    fn apply_native_choice(&mut self, idx: usize) {
        if idx == 0 {
            match self.config.clear_native() {
                Ok(()) => {
                    self.refresh_display();
                    let label = self
                        .display
                        .panel
                        .map(Mode::label)
                        .unwrap_or_else(|| "panel".into());
                    self.set_ok(format!("Native default → auto ({label})"));
                    self.screen = Screen::Home;
                }
                Err(e) => self.set_err(format!("Could not clear native: {e}")),
            }
            return;
        }
        let Some(mode) = self.native_modes.get(idx - 1).copied() else {
            return;
        };
        match self
            .config
            .set_native(mode.width, mode.height, Some(mode.refresh))
        {
            Ok(()) => {
                self.refresh_display();
                self.set_ok(format!("Native default → {} (saved)", mode.label()));
                self.screen = Screen::Home;
            }
            Err(e) => self.set_err(format!("Could not save native: {e}")),
        }
    }

    fn apply_preset(&mut self, idx: usize) {
        let Some(preset) = POPULAR_STRETCH.get(idx) else {
            return;
        };
        match self.config.set_stretch(preset.width, preset.height) {
            Ok(name) => {
                self.set_ok(format!("Default stretch → {name}"));
                self.screen = Screen::Home;
            }
            Err(e) => self.set_err(format!("Could not save default: {e}")),
        }
    }

    pub(super) fn on_key(&mut self, code: KeyCode) {
        if self.confirm_update {
            match code {
                KeyCode::Enter => {
                    self.confirm_update = false;
                    if let UpdateState::Available(release) = &self.update {
                        let release = release.clone();
                        self.update = UpdateState::Installing {
                            version: release.version.to_string(),
                            result: background(move || update::install(&release)),
                        };
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => self.confirm_update = false,
                _ => {}
            }
            return;
        }
        if code == KeyCode::Char('u') {
            match self.update {
                UpdateState::Available(_) => self.confirm_update = true,
                UpdateState::Current | UpdateState::Failed(_) => {
                    self.update = UpdateState::Checking(background(update::check));
                }
                _ => {}
            }
            return;
        }
        match self.screen {
            Screen::Home => self.on_home_key(code),
            Screen::Presets => self.on_preset_key(code),
            Screen::Native => self.on_native_key(code),
        }
        if self.should_quit && matches!(self.update, UpdateState::Installing { .. }) {
            self.should_quit = false;
            self.set_ok("Finishing the update. You can quit when it completes.");
        }
    }

    pub(super) fn poll_update(&mut self) {
        let next = match &self.update {
            UpdateState::Checking(result) => match result.try_recv() {
                Ok(Ok(Some(release))) => Some(UpdateState::Available(release)),
                Ok(Ok(None)) => Some(UpdateState::Current),
                Ok(Err(error)) => Some(UpdateState::Failed(error)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(UpdateState::Failed("Update check stopped".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            },
            UpdateState::Installing { version, result } => match result.try_recv() {
                Ok(Ok(())) => Some(UpdateState::Installed(version.clone())),
                Ok(Err(error)) => Some(UpdateState::Failed(error)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(UpdateState::Failed("Update installation stopped".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            },
            _ => None,
        };
        if let Some(next) = next {
            self.update = next;
        }
    }

    fn on_home_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.home_state.selected().unwrap_or(0);
                let next = (i + 1) % HOME_ACTIONS.len();
                self.home_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.home_state.selected().unwrap_or(0);
                let next = if i == 0 {
                    HOME_ACTIONS.len() - 1
                } else {
                    i - 1
                };
                self.home_state.select(Some(next));
            }
            KeyCode::Char('1') => self.run_toggle(),
            KeyCode::Char('2') => self.run_stretch(),
            KeyCode::Char('3') => self.run_native(),
            KeyCode::Char('4') => {
                self.screen = Screen::Presets;
            }
            KeyCode::Char('5') => self.open_native_picker(),
            KeyCode::Enter => {
                let i = self.home_state.selected().unwrap_or(0);
                match HOME_ACTIONS[i].0 {
                    HomeAction::Toggle => self.run_toggle(),
                    HomeAction::Stretch => self.run_stretch(),
                    HomeAction::Native => self.run_native(),
                    HomeAction::ChangeStretch => self.screen = Screen::Presets,
                    HomeAction::ChangeNative => self.open_native_picker(),
                    HomeAction::Quit => self.should_quit = true,
                }
            }
            KeyCode::Char('r') => {
                self.refresh_display();
                self.set_ok("Refreshed display info");
            }
            _ => {}
        }
    }

    fn on_preset_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                self.screen = Screen::Home;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.preset_state.selected().unwrap_or(0);
                let next = (i + 1) % POPULAR_STRETCH.len();
                self.preset_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.preset_state.selected().unwrap_or(0);
                let next = if i == 0 {
                    POPULAR_STRETCH.len() - 1
                } else {
                    i - 1
                };
                self.preset_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(i) = self.preset_state.selected() {
                    self.apply_preset(i);
                }
            }
            _ => {}
        }
    }

    fn on_native_key(&mut self, code: KeyCode) {
        // The Auto row is always present, even when Windows reports no modes.
        let len = self.native_choice_len();
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                self.screen = Screen::Home;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.native_state.selected().unwrap_or(0);
                self.native_state.select(Some((i + 1) % len));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.native_state.selected().unwrap_or(0);
                let next = if i == 0 { len - 1 } else { i - 1 };
                self.native_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(i) = self.native_state.selected() {
                    self.apply_native_choice(i);
                }
            }
            KeyCode::Char('r') => {
                self.refresh_display();
                self.open_native_picker();
                self.set_ok("Refreshed display modes");
            }
            _ => {}
        }
    }
}
