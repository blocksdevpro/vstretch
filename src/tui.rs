//! Terminal controls for profiles, manual display changes, and verified updates.

use std::io::{self, stdout};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::config::{Config, POPULAR_STRETCH};
use crate::display::{self, Mode, ModeKind};
use crate::update::{self, Release};

type UpdateResult<T> = Receiver<std::result::Result<T, String>>;

enum UpdateState {
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
enum Screen {
    Home,
    Presets,
    Native,
}

#[derive(Clone, Copy)]
enum HomeAction {
    Toggle,
    Stretch,
    Native,
    ChangeStretch,
    ChangeNative,
    Quit,
}

const HOME_ACTIONS: &[(HomeAction, &str)] = &[
    (HomeAction::Toggle, "Toggle  native ↔ stretch"),
    (HomeAction::Stretch, "Apply stretch (default profile)"),
    (HomeAction::Native, "Apply native"),
    (HomeAction::ChangeStretch, "Change default stretch…"),
    (HomeAction::ChangeNative, "Change default native…"),
    (HomeAction::Quit, "Quit"),
];

struct App {
    screen: Screen,
    home_state: ListState,
    preset_state: ListState,
    native_state: ListState,
    native_modes: Vec<Mode>,
    config: Config,
    current: Option<Mode>,
    panel: Option<Mode>,
    native: Option<Mode>,
    status: String,
    status_ok: bool,
    should_quit: bool,
    update: UpdateState,
    confirm_update: bool,
}

impl App {
    fn new() -> Result<Self> {
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
            current: None,
            panel: None,
            native: None,
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
        self.current = display::get_current_resolution().ok();
        self.panel = display::get_native_resolution().ok();
        self.native = display::resolve_native(self.config.native.as_ref(), self.panel).ok();
    }

    fn set_ok(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_ok = true;
    }

    fn set_err(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_ok = false;
    }

    fn stretch_label(&self) -> String {
        self.config.stretch.name()
    }

    fn mode_label(&self) -> (&'static str, Color) {
        match display::classify_mode(
            self.current,
            self.native,
            self.panel,
            Some(&self.config.stretch),
        ) {
            ModeKind::Native => ("NATIVE", Color::Cyan),
            ModeKind::Stretch => ("STRETCH", Color::Magenta),
            ModeKind::Panel => ("PANEL", Color::Blue),
            ModeKind::Other => ("OTHER", Color::DarkGray),
            ModeKind::Unknown => ("UNKNOWN", Color::DarkGray),
        }
    }

    fn effective_native(&self) -> Result<Mode> {
        self.native.context("could not resolve native resolution")
    }

    fn run_toggle(&mut self) {
        let native = match self.effective_native() {
            Ok(n) => n,
            Err(e) => {
                self.set_err(format!("Could not read native: {e}"));
                return;
            }
        };
        let name = self.config.stretch.name();
        match display::toggle_stretch(
            &self.config.stretch,
            native,
            display::stretch_refresh(self.panel, Some(native)),
        ) {
            Ok(mode) => {
                self.set_ok(format!("Toggled → {}  [{name}]", mode.label()));
                self.refresh_display();
            }
            Err(e) => self.set_err(format!("Toggle failed: {e}")),
        }
    }

    fn run_stretch(&mut self) {
        let name = self.config.stretch.name();
        let refresh = display::stretch_refresh(self.panel, self.native);
        match display::apply_profile(&self.config.stretch, refresh) {
            Ok(mode) => {
                self.set_ok(format!("Stretch → {}  [{name}]", mode.label()));
                self.refresh_display();
            }
            Err(e) => self.set_err(format!("Stretch failed: {e}")),
        }
    }

    fn run_native(&mut self) {
        match self.effective_native() {
            Ok(native) => match display::change_resolution(native) {
                Ok(()) => {
                    self.set_ok(format!("Native → {}", native.label()));
                    self.refresh_display();
                }
                Err(e) => self.set_err(format!("Native failed: {e}")),
            },
            Err(e) => self.set_err(format!("Could not read native: {e}")),
        }
    }

    fn native_choice_len(&self) -> usize {
        self.native_modes.len() + 1
    }

    fn open_native_picker(&mut self) {
        self.native_modes = display::list_display_modes();
        if let Some(over) = &self.config.native
            && display::preferred_override_mode(&self.native_modes, over, self.panel).is_none()
        {
            self.native_modes
                .insert(0, display::native_override_mode(over, self.panel));
        }
        let idx = match &self.config.native {
            None => 0,
            Some(over) => display::preferred_override_mode(&self.native_modes, over, self.panel)
                .and_then(|pref| self.native_modes.iter().position(|m| *m == pref))
                .map(|i| i + 1)
                .unwrap_or(0),
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

    fn on_key(&mut self, code: KeyCode) {
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

    fn poll_update(&mut self) {
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

struct TerminalSession;

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // Cleanup must also run when entering the screen or creating the backend
        // fails. Ignore cleanup errors so the original failure stays visible.
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    }
}

pub fn run() -> Result<()> {
    let mut app = App::new()?;
    enable_raw_mode().context("enable raw mode")?;
    let _session = TerminalSession;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    run_loop(&mut terminal, &mut app)
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        app.poll_update();
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key.code);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, chunks[0]);
    draw_status_panel(f, app, chunks[1]);

    match app.screen {
        Screen::Home => draw_home_menu(f, app, chunks[2]),
        Screen::Presets => draw_presets(f, app, chunks[2]),
        Screen::Native => draw_native_modes(f, app, chunks[2]),
    }

    draw_message(f, app, chunks[3]);
    draw_update(f, app, chunks[4]);
    draw_help(f, app, chunks[5]);
    if app.confirm_update {
        draw_update_confirmation(f, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " vstretch ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::default().fg(Color::White),
        ),
        Span::styled("native ↔ stretch", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(title, area);
}

fn draw_update(f: &mut Frame, app: &App, area: Rect) {
    let (text, color) = match &app.update {
        UpdateState::Checking(_) => ("Checking for updates...".into(), Color::DarkGray),
        UpdateState::Current => ("Up to date.  u  check again".into(), Color::DarkGray),
        UpdateState::Available(release) => (
            format!("v{} is available!  u  update", release.version),
            Color::Yellow,
        ),
        UpdateState::Failed(error) => (format!("{error}  |  u  retry"), Color::Red),
        UpdateState::Installing { version, .. } => {
            (format!("Installing v{version}..."), Color::Cyan)
        }
        UpdateState::Installed(version) => (
            format!("Updated to v{version}. Quit and reopen vstretch."),
            Color::Green,
        ),
    };
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .title(" updates ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color)),
            ),
        area,
    );
}

fn draw_update_confirmation(f: &mut Frame, app: &App) {
    let UpdateState::Available(release) = &app.update else {
        return;
    };
    let area = f.area();
    let width = area.width.min(62);
    let height = area.height.min(8);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, popup);
    let message = format!(
        "Install vstretch v{}?\n\n\
         Download and replace this executable. Your display config stays saved. \
         Reopen vstretch after updating.",
        release.version
    );
    let confirmation = Paragraph::new(message).wrap(Wrap { trim: true }).block(
        Block::default()
            .title(" Update available ")
            .title_bottom(" Enter  install    Esc  later ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(confirmation, popup);
}

fn draw_status_panel(f: &mut Frame, app: &App, area: Rect) {
    let (mode_name, mode_color) = app.mode_label();
    let current = app.current.map(Mode::label).unwrap_or_else(|| "—".into());
    let native = match app.native {
        Some(n) => {
            let src = if app.config.native.is_some() {
                "set"
            } else {
                "auto"
            };
            format!("{}  ({})", n.label(), src)
        }
        None => "—".into(),
    };
    let default = app.stretch_label();

    let lines = vec![
        Line::from(vec![
            Span::styled("  Mode      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                mode_name,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Current   ", Style::default().fg(Color::DarkGray)),
            Span::styled(current, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Native    ", Style::default().fg(Color::DarkGray)),
            Span::styled(native, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Stretch   ", Style::default().fg(Color::DarkGray)),
            Span::styled(default, Style::default().fg(Color::Yellow)),
        ]),
    ];

    let panel = Paragraph::new(lines).block(
        Block::default()
            .title(" display ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(panel, area);
}

fn draw_home_menu(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = HOME_ACTIONS
        .iter()
        .enumerate()
        .map(|(i, (_, label))| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}. ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(*label),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" actions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.home_state);
}

fn draw_presets(f: &mut Frame, app: &mut App, area: Rect) {
    let default = (app.config.stretch.width, app.config.stretch.height);

    let items: Vec<ListItem> = POPULAR_STRETCH
        .iter()
        .map(|p| {
            let is_default = default == (p.width, p.height);
            let mut spans = vec![
                Span::styled(
                    p.resolution_label(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("    {:5}", p.aspect),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            // Reserve the tag column so the saved-preset star lines up on every row.
            let (tag, tag_color) = match p.tag {
                Some("popular") => ("popular", Color::Magenta),
                Some("common") => ("common ", Color::Cyan),
                _ => ("       ", Color::DarkGray),
            };
            spans.push(Span::styled(
                format!("    {tag}"),
                Style::default().fg(tag_color),
            ));
            if is_default {
                spans.push(Span::styled(
                    "  ★",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select default stretch resolution ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.preset_state);
}

fn draw_native_modes(f: &mut Frame, app: &mut App, area: Rect) {
    let mut items = Vec::with_capacity(app.native_choice_len());

    let auto_is_default = app.config.native.is_none();
    let starred = app
        .config
        .native
        .as_ref()
        .and_then(|n| display::preferred_override_mode(&app.native_modes, n, app.panel));
    let mut auto_spans = vec![Span::styled(
        "Auto (panel native)",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(panel) = app.panel {
        auto_spans.push(Span::styled(
            format!("    {}", panel.label()),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if auto_is_default {
        auto_spans.push(Span::styled(
            "  ★",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    items.push(ListItem::new(Line::from(auto_spans)));

    for mode in &app.native_modes {
        let is_default = starred == Some(*mode);
        let mut spans = vec![
            Span::styled(
                format!("{:>4} × {:<4}", mode.width, mode.height),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("    {:>3}Hz", mode.refresh),
                Style::default().fg(Color::DarkGray),
            ),
        ];
        if app
            .panel
            .is_some_and(|p| p.size_eq(*mode) && p.refresh == mode.refresh)
        {
            spans.push(Span::styled("    panel", Style::default().fg(Color::Cyan)));
        } else if app.current.is_some_and(|c| c == *mode) {
            spans.push(Span::styled("    now", Style::default().fg(Color::Magenta)));
        }
        if is_default {
            spans.push(Span::styled(
                "  ★",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select default native resolution ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.native_state);
}

fn draw_message(f: &mut Frame, app: &App, area: Rect) {
    let color = if app.status_ok {
        Color::Green
    } else {
        Color::Red
    };
    let msg = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(&app.status, Style::default().fg(color)),
    ]))
    .block(
        Block::default()
            .title(" status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    f.render_widget(msg, area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.screen {
        Screen::Home => "↑↓/jk  move   Enter  select   1–5  quick   r  refresh   q  quit",
        Screen::Presets => "↑↓/jk  move   Enter  set default   Esc  back",
        Screen::Native => "↑↓/jk  move   Enter  set default   r  refresh   Esc  back",
    };
    let help = Paragraph::new(Span::styled(
        format!("  {text}"),
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(help, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn app(update: UpdateState) -> App {
        App {
            screen: Screen::Home,
            home_state: ListState::default(),
            preset_state: ListState::default(),
            native_state: ListState::default(),
            native_modes: Vec::new(),
            config: Config::default(),
            current: None,
            panel: None,
            native: None,
            status: "Ready".into(),
            status_ok: true,
            should_quit: false,
            update,
            confirm_update: false,
        }
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn available_update_renders_banner_and_confirmation_can_be_dismissed() {
        let mut app = app(UpdateState::Available(update::tests::available_release()));
        assert!(render(&mut app, 90, 30).contains("v9.0.0 is available!"));
        app.on_key(KeyCode::Char('u'));
        assert!(app.confirm_update);
        let popup = render(&mut app, 90, 30);
        assert!(popup.contains("Install vstretch v9.0.0?"));
        assert!(popup.contains("Enter  install    Esc  later"));
        app.on_key(KeyCode::Esc);
        assert!(!app.confirm_update);
        assert!(matches!(app.update, UpdateState::Available(_)));
        assert!(!app.should_quit);
    }

    #[test]
    fn check_results_and_offline_errors_reach_the_banner() {
        let (sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Checking(receiver));
        sender
            .send(Ok(Some(update::tests::available_release())))
            .unwrap();
        app.poll_update();
        assert!(matches!(app.update, UpdateState::Available(_)));
        app.update = UpdateState::Failed("Offline".into());
        assert!(render(&mut app, 90, 30).contains("Offline  |  u  retry"));
        app.on_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn installation_blocks_quit_then_shows_restart_notice() {
        let (sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Installing {
            version: "9.0.0".into(),
            result: receiver,
        });
        app.on_key(KeyCode::Char('q'));
        assert!(!app.should_quit);
        app.home_state.select(Some(HOME_ACTIONS.len() - 1));
        app.on_key(KeyCode::Enter);
        assert!(!app.should_quit);
        sender.send(Ok(())).unwrap();
        app.poll_update();
        assert!(render(&mut app, 90, 30).contains("Updated to v9.0.0. Quit and reopen vstretch."));
        app.on_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn installation_allows_leaving_resolution_pickers() {
        let (_sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Installing {
            version: "9.0.0".into(),
            result: receiver,
        });
        for screen in [Screen::Presets, Screen::Native] {
            for key in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Backspace] {
                app.screen = screen;
                app.on_key(key);
                assert!(matches!(app.screen, Screen::Home));
                assert!(!app.should_quit);
                assert!(matches!(app.update, UpdateState::Installing { .. }));
            }
        }
    }

    #[test]
    fn update_rendering_handles_small_terminals() {
        let mut app = app(UpdateState::Available(update::tests::available_release()));
        app.confirm_update = true;
        let popup = render(&mut app, 35, 15);
        assert!(popup.contains("Install vstretch v9.0.0?"));
        assert!(popup.contains("Enter  install    Esc  later"));
    }
}
