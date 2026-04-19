use std::cell::RefCell;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::TableState;

use crate::auth::ResolvedAuth;
use crate::browser::open_target;
use crate::config::EffectiveConfig;
use crate::github::GitHubClient;
use crate::model::{
    DashboardState, DetailTarget, FocusPane, PullRequestSummary, WorkflowRunSummary,
};
use crate::poller::{DashboardUpdate, PollerControl, PollerMessage, apply_update, spawn_poller};
use crate::ui;

pub struct App {
    pub config: EffectiveConfig,
    pub state: DashboardState,
    pub focus: FocusPane,
    pub show_help: bool,
    pub detail_open: bool,
    pub detail_scroll: usize,
    pub spinner_index: usize,
    pub actions_table_state: RefCell<TableState>,
    pub prs_table_state: RefCell<TableState>,
    pub selected_action_id: Option<u64>,
    pub selected_pr_id: Option<String>,
    pub actions_index: usize,
    pub prs_index: usize,
}

impl App {
    fn new(config: EffectiveConfig) -> Self {
        Self {
            config,
            state: DashboardState::default(),
            focus: FocusPane::Actions,
            show_help: false,
            detail_open: false,
            detail_scroll: 0,
            spinner_index: 0,
            actions_table_state: RefCell::new(TableState::default()),
            prs_table_state: RefCell::new(TableState::default()),
            selected_action_id: None,
            selected_pr_id: None,
            actions_index: 0,
            prs_index: 0,
        }
    }

    fn open_target(&self) -> Option<&str> {
        if self.detail_open {
            return self
                .state
                .detail
                .as_ref()
                .map(|detail| detail.summary.url.as_str());
        }
        match self.focus {
            FocusPane::Actions => self.current_action().map(|run| run.url.as_str()),
            FocusPane::PullRequests => self.current_pr().map(|pr| pr.url.as_str()),
        }
    }

    fn current_action(&self) -> Option<&WorkflowRunSummary> {
        self.state.actions.get(self.actions_index)
    }

    fn current_pr(&self) -> Option<&PullRequestSummary> {
        self.state.pulls.get(self.prs_index)
    }

    fn move_selection(&mut self, delta: i32) {
        if self.detail_open {
            self.detail_scroll = self.detail_scroll.saturating_add_signed(delta as isize);
            return;
        }

        match self.focus {
            FocusPane::Actions => {
                if self.state.actions.is_empty() {
                    return;
                }
                let max = self.state.actions.len().saturating_sub(1);
                self.actions_index = self
                    .actions_index
                    .saturating_add_signed(delta as isize)
                    .min(max);
                self.selected_action_id =
                    self.state.actions.get(self.actions_index).map(|run| run.id);
                self.actions_table_state
                    .borrow_mut()
                    .select(Some(self.actions_index));
            }
            FocusPane::PullRequests => {
                if self.state.pulls.is_empty() {
                    return;
                }
                let max = self.state.pulls.len().saturating_sub(1);
                self.prs_index = self
                    .prs_index
                    .saturating_add_signed(delta as isize)
                    .min(max);
                self.selected_pr_id = self
                    .state
                    .pulls
                    .get(self.prs_index)
                    .map(|pr| pr.stable_id());
                self.prs_table_state
                    .borrow_mut()
                    .select(Some(self.prs_index));
            }
        }
    }

    fn jump_to(&mut self, bottom: bool) {
        if self.detail_open {
            self.detail_scroll = if bottom { usize::MAX / 4 } else { 0 };
            return;
        }

        match self.focus {
            FocusPane::Actions if !self.state.actions.is_empty() => {
                self.actions_index = if bottom {
                    self.state.actions.len() - 1
                } else {
                    0
                };
                self.selected_action_id =
                    self.state.actions.get(self.actions_index).map(|run| run.id);
                self.actions_table_state
                    .borrow_mut()
                    .select(Some(self.actions_index));
            }
            FocusPane::PullRequests if !self.state.pulls.is_empty() => {
                self.prs_index = if bottom {
                    self.state.pulls.len() - 1
                } else {
                    0
                };
                self.selected_pr_id = self
                    .state
                    .pulls
                    .get(self.prs_index)
                    .map(|pr| pr.stable_id());
                self.prs_table_state
                    .borrow_mut()
                    .select(Some(self.prs_index));
            }
            _ => {}
        }
    }

    fn anchor_selection(&mut self) {
        if !self.state.actions.is_empty() {
            if let Some(selected_id) = self.selected_action_id {
                if let Some(index) = self
                    .state
                    .actions
                    .iter()
                    .position(|run| run.id == selected_id)
                {
                    self.actions_index = index;
                } else {
                    self.actions_index = self.actions_index.min(self.state.actions.len() - 1);
                    self.selected_action_id =
                        self.state.actions.get(self.actions_index).map(|run| run.id);
                }
            } else {
                self.actions_index = 0;
                self.selected_action_id = self.state.actions.first().map(|run| run.id);
            }
            self.actions_table_state
                .borrow_mut()
                .select(Some(self.actions_index));
        } else {
            self.actions_index = 0;
            self.selected_action_id = None;
            self.actions_table_state.borrow_mut().select(None);
        }

        if !self.state.pulls.is_empty() {
            if let Some(selected_id) = &self.selected_pr_id {
                if let Some(index) = self
                    .state
                    .pulls
                    .iter()
                    .position(|pr| pr.stable_id() == *selected_id)
                {
                    self.prs_index = index;
                } else {
                    self.prs_index = self.prs_index.min(self.state.pulls.len() - 1);
                    self.selected_pr_id = self
                        .state
                        .pulls
                        .get(self.prs_index)
                        .map(|pr| pr.stable_id());
                }
            } else {
                self.prs_index = 0;
                self.selected_pr_id = self.state.pulls.first().map(|pr| pr.stable_id());
            }
            self.prs_table_state
                .borrow_mut()
                .select(Some(self.prs_index));
        } else {
            self.prs_index = 0;
            self.selected_pr_id = None;
            self.prs_table_state.borrow_mut().select(None);
        }
    }

    fn apply_update(&mut self, update: DashboardUpdate) {
        apply_update(&mut self.state, update);
        if !self.state.errors.is_empty() {
            self.state.errors.sort();
        }
        self.anchor_selection();
    }
}

pub fn run_app(config: EffectiveConfig, auth: ResolvedAuth) -> Result<()> {
    let client = GitHubClient::new(&config.host, &auth.token)?;
    let viewer_login = client.viewer_login().unwrap_or_else(|_| String::new());

    let (sender, receiver) = mpsc::channel();
    let poller_control = spawn_poller(config.clone(), client, viewer_login, sender);

    let mut app = App::new(config);
    let mut terminal = init_terminal()?;
    let _ctrlc = install_ctrlc_handler(Arc::clone(&poller_control));
    install_panic_hook();

    loop {
        if poller_control
            .stop
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        }
        terminal.draw(|frame| ui::draw(frame, &app))?;

        while let Ok(message) = receiver.try_recv() {
            let PollerMessage::Update(update) = message;
            app.apply_update(update);
        }

        app.spinner_index = app.spinner_index.wrapping_add(1);

        if event::poll(Duration::from_millis(125)).context("failed to poll terminal events")?
            && let Event::Key(key) = event::read().context("failed to read terminal event")?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('?') => {
                    app.show_help = !app.show_help;
                }
                KeyCode::Esc => {
                    if app.show_help {
                        app.show_help = false;
                    } else if app.detail_open {
                        app.detail_open = false;
                        app.detail_scroll = 0;
                        poller_control.set_detail_target(None);
                    }
                }
                KeyCode::Tab if !app.detail_open => {
                    app.focus = app.focus.toggle();
                }
                KeyCode::Char('r') => {
                    poller_control.request_refresh();
                }
                KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
                KeyCode::Char('g') => app.jump_to(false),
                KeyCode::Char('G') => app.jump_to(true),
                KeyCode::Char('l')
                    if !app.show_help && !app.detail_open && app.focus == FocusPane::Actions =>
                {
                    if let Some(run) = app.current_action().cloned() {
                        app.detail_open = true;
                        app.detail_scroll = 0;
                        poller_control.set_detail_target(Some(DetailTarget {
                            repo: run.repo.clone(),
                            run_id: run.id,
                        }));
                    }
                }
                KeyCode::Enter | KeyCode::Char('o') if !app.show_help => {
                    if let Some(target) = app.open_target() {
                        let _ = open_target(target, app.config.ui.open_command.as_deref());
                    }
                }
                _ => {}
            }
        }
    }

    poller_control.stop();
    restore_terminal(&mut terminal)
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal backend")?;
    terminal.hide_cursor().context("failed to hide cursor")?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

fn install_ctrlc_handler(control: Arc<PollerControl>) -> Result<()> {
    ctrlc::set_handler(move || {
        control.stop();
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
    })
    .context("failed to install ctrl-c handler")
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
        default_hook(panic);
    }));
}
