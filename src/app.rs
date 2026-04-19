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
    DashboardState, DetailTarget, DetailView, FocusPane, Mode, PullRequestSummary, RepoTarget,
    WorkflowRunSummary,
};
use crate::poller::{DashboardUpdate, PollerControl, PollerMessage, apply_update, spawn_poller};
use crate::ui;

pub struct SplitPaneState {
    pub content: FocusPane,
    pub table_state: RefCell<TableState>,
    pub selected_action_id: Option<u64>,
    pub selected_pr_id: Option<String>,
    pub actions_index: usize,
    pub prs_index: usize,
    pub detail_target: Option<DetailTarget>,
    pub detail_scroll: usize,
}

impl SplitPaneState {
    fn new(content: FocusPane) -> Self {
        Self {
            content,
            table_state: RefCell::new(TableState::default()),
            selected_action_id: None,
            selected_pr_id: None,
            actions_index: 0,
            prs_index: 0,
            detail_target: None,
            detail_scroll: 0,
        }
    }
}

pub struct App {
    pub config: EffectiveConfig,
    pub state: DashboardState,
    pub compact_focus: FocusPane,
    pub split_focus: usize,
    pub show_help: bool,
    pub compact_detail_target: Option<DetailTarget>,
    pub compact_detail_scroll: usize,
    pub spinner_index: usize,
    pub actions_table_state: RefCell<TableState>,
    pub prs_table_state: RefCell<TableState>,
    pub selected_action_id: Option<u64>,
    pub selected_pr_id: Option<String>,
    pub actions_index: usize,
    pub prs_index: usize,
    pub split_panes: [SplitPaneState; 2],
}

impl App {
    fn new(config: EffectiveConfig) -> Self {
        Self {
            config,
            state: DashboardState::default(),
            compact_focus: FocusPane::Actions,
            split_focus: 0,
            show_help: false,
            compact_detail_target: None,
            compact_detail_scroll: 0,
            spinner_index: 0,
            actions_table_state: RefCell::new(TableState::default()),
            prs_table_state: RefCell::new(TableState::default()),
            selected_action_id: None,
            selected_pr_id: None,
            actions_index: 0,
            prs_index: 0,
            split_panes: [
                SplitPaneState::new(FocusPane::Actions),
                SplitPaneState::new(FocusPane::Actions),
            ],
        }
    }

    pub(crate) fn split_repo(&self, pane_index: usize) -> Option<&RepoTarget> {
        self.config.repos.get(pane_index)
    }

    pub(crate) fn split_actions(&self, pane_index: usize) -> Vec<&WorkflowRunSummary> {
        let Some(repo) = self.split_repo(pane_index) else {
            return Vec::new();
        };
        let slug = repo.slug();
        self.state
            .actions
            .iter()
            .filter(|run| run.repo.slug() == slug)
            .collect()
    }

    pub(crate) fn split_pulls(&self, pane_index: usize) -> Vec<&PullRequestSummary> {
        let Some(repo) = self.split_repo(pane_index) else {
            return Vec::new();
        };
        let slug = repo.slug();
        self.state
            .pulls
            .iter()
            .filter(|pr| pr.repo.slug() == slug)
            .collect()
    }

    pub(crate) fn split_detail_open(&self, pane_index: usize) -> bool {
        self.config.mode == Mode::Split && self.split_panes[pane_index].detail_target.is_some()
    }

    pub(crate) fn detail_open(&self) -> bool {
        match self.config.mode {
            Mode::Compact => self.compact_detail_target.is_some(),
            Mode::Split => self
                .split_panes
                .iter()
                .any(|pane| pane.detail_target.is_some()),
        }
    }

    pub(crate) fn focused_view(&self) -> FocusPane {
        match self.config.mode {
            Mode::Compact => self.compact_focus,
            Mode::Split => self.split_panes[self.split_focus].content,
        }
    }

    pub(crate) fn current_repo_label(&self) -> Option<String> {
        match self.config.mode {
            Mode::Compact => None,
            Mode::Split => self.split_repo(self.split_focus).map(RepoTarget::slug),
        }
    }

    pub(crate) fn current_detail_target(&self) -> Option<&DetailTarget> {
        match self.config.mode {
            Mode::Compact => self.compact_detail_target.as_ref(),
            Mode::Split => self.split_panes[self.split_focus].detail_target.as_ref(),
        }
    }

    pub(crate) fn split_detail_target(&self, pane_index: usize) -> Option<&DetailTarget> {
        self.split_panes[pane_index].detail_target.as_ref()
    }

    pub(crate) fn detail_scroll_for(&self, pane_index: Option<usize>) -> usize {
        match self.config.mode {
            Mode::Compact => self.compact_detail_scroll,
            Mode::Split => pane_index
                .and_then(|index| self.split_panes.get(index))
                .map(|pane| pane.detail_scroll)
                .unwrap_or_default(),
        }
    }

    pub(crate) fn detail_view(&self, target: &DetailTarget) -> Option<&DetailView> {
        self.state.detail_cache.get(&target.cache_key())
    }

    fn active_detail_targets(&self) -> Vec<DetailTarget> {
        match self.config.mode {
            Mode::Compact => self.compact_detail_target.clone().into_iter().collect(),
            Mode::Split => self
                .split_panes
                .iter()
                .filter_map(|pane| pane.detail_target.clone())
                .collect(),
        }
    }

    fn sync_detail_targets(&self, poller_control: &PollerControl) {
        poller_control.set_detail_targets(self.active_detail_targets());
    }

    fn open_target(&self) -> Option<&str> {
        if let Some(target) = self.current_detail_target() {
            return self.detail_view(target).map(DetailView::url).or_else(|| {
                match self.focused_view() {
                    FocusPane::Actions => self.current_action().map(|run| run.url.as_str()),
                    FocusPane::PullRequests => self.current_pr().map(|pr| pr.url.as_str()),
                }
            });
        }
        match self.focused_view() {
            FocusPane::Actions => self.current_action().map(|run| run.url.as_str()),
            FocusPane::PullRequests => self.current_pr().map(|pr| pr.url.as_str()),
        }
    }

    pub(crate) fn current_action(&self) -> Option<&WorkflowRunSummary> {
        match self.config.mode {
            Mode::Compact => self.state.actions.get(self.actions_index),
            Mode::Split => self.current_split_action(self.split_focus),
        }
    }

    pub(crate) fn current_pr(&self) -> Option<&PullRequestSummary> {
        match self.config.mode {
            Mode::Compact => self.state.pulls.get(self.prs_index),
            Mode::Split => self.current_split_pr(self.split_focus),
        }
    }

    pub(crate) fn current_split_action(&self, pane_index: usize) -> Option<&WorkflowRunSummary> {
        let index = self.split_panes[pane_index].actions_index;
        self.split_actions(pane_index).into_iter().nth(index)
    }

    pub(crate) fn current_split_pr(&self, pane_index: usize) -> Option<&PullRequestSummary> {
        let index = self.split_panes[pane_index].prs_index;
        self.split_pulls(pane_index).into_iter().nth(index)
    }

    fn move_selection(&mut self, delta: i32) {
        match self.config.mode {
            Mode::Compact => self.move_compact_selection(delta),
            Mode::Split => self.move_split_selection(delta),
        }
    }

    fn move_compact_selection(&mut self, delta: i32) {
        if self.compact_detail_target.is_some() {
            self.compact_detail_scroll = self
                .compact_detail_scroll
                .saturating_add_signed(delta as isize);
            return;
        }

        match self.compact_focus {
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

    fn move_split_selection(&mut self, delta: i32) {
        if self.split_panes[self.split_focus].detail_target.is_some() {
            self.split_panes[self.split_focus].detail_scroll = self.split_panes[self.split_focus]
                .detail_scroll
                .saturating_add_signed(delta as isize);
            return;
        }

        let pane_index = self.split_focus;
        match self.split_panes[pane_index].content {
            FocusPane::Actions => {
                let row_ids = self
                    .split_actions(pane_index)
                    .into_iter()
                    .map(|run| run.id)
                    .collect::<Vec<_>>();
                if row_ids.is_empty() {
                    return;
                }
                let max = row_ids.len().saturating_sub(1);
                self.split_panes[pane_index].actions_index = self.split_panes[pane_index]
                    .actions_index
                    .saturating_add_signed(delta as isize)
                    .min(max);
                self.split_panes[pane_index].selected_action_id = row_ids
                    .get(self.split_panes[pane_index].actions_index)
                    .copied();
                self.sync_split_table_state(pane_index);
            }
            FocusPane::PullRequests => {
                let row_ids = self
                    .split_pulls(pane_index)
                    .into_iter()
                    .map(PullRequestSummary::stable_id)
                    .collect::<Vec<_>>();
                if row_ids.is_empty() {
                    return;
                }
                let max = row_ids.len().saturating_sub(1);
                self.split_panes[pane_index].prs_index = self.split_panes[pane_index]
                    .prs_index
                    .saturating_add_signed(delta as isize)
                    .min(max);
                self.split_panes[pane_index].selected_pr_id =
                    row_ids.get(self.split_panes[pane_index].prs_index).cloned();
                self.sync_split_table_state(pane_index);
            }
        }
    }

    fn jump_to(&mut self, bottom: bool) {
        match self.config.mode {
            Mode::Compact => self.jump_compact(bottom),
            Mode::Split => self.jump_split(bottom),
        }
    }

    fn jump_compact(&mut self, bottom: bool) {
        if self.compact_detail_target.is_some() {
            self.compact_detail_scroll = if bottom { usize::MAX / 4 } else { 0 };
            return;
        }

        match self.compact_focus {
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

    fn jump_split(&mut self, bottom: bool) {
        if self.split_panes[self.split_focus].detail_target.is_some() {
            self.split_panes[self.split_focus].detail_scroll =
                if bottom { usize::MAX / 4 } else { 0 };
            return;
        }

        let pane_index = self.split_focus;
        match self.split_panes[pane_index].content {
            FocusPane::Actions => {
                let row_ids = self
                    .split_actions(pane_index)
                    .into_iter()
                    .map(|run| run.id)
                    .collect::<Vec<_>>();
                if row_ids.is_empty() {
                    return;
                }
                self.split_panes[pane_index].actions_index =
                    if bottom { row_ids.len() - 1 } else { 0 };
                self.split_panes[pane_index].selected_action_id = row_ids
                    .get(self.split_panes[pane_index].actions_index)
                    .copied();
            }
            FocusPane::PullRequests => {
                let row_ids = self
                    .split_pulls(pane_index)
                    .into_iter()
                    .map(PullRequestSummary::stable_id)
                    .collect::<Vec<_>>();
                if row_ids.is_empty() {
                    return;
                }
                self.split_panes[pane_index].prs_index = if bottom { row_ids.len() - 1 } else { 0 };
                self.split_panes[pane_index].selected_pr_id =
                    row_ids.get(self.split_panes[pane_index].prs_index).cloned();
            }
        }
        self.sync_split_table_state(pane_index);
    }

    fn open_detail(&mut self, poller_control: &PollerControl) {
        let target = match self.focused_view() {
            FocusPane::Actions => {
                self.current_action()
                    .cloned()
                    .map(|run| DetailTarget::WorkflowRun {
                        repo: run.repo.clone(),
                        run_id: run.id,
                    })
            }
            FocusPane::PullRequests => {
                self.current_pr()
                    .cloned()
                    .map(|pr| DetailTarget::PullRequest {
                        repo: pr.repo.clone(),
                        number: pr.number,
                    })
            }
        };
        if let Some(target) = target {
            match self.config.mode {
                Mode::Compact => {
                    self.compact_detail_target = Some(target);
                    self.compact_detail_scroll = 0;
                }
                Mode::Split => {
                    let pane = &mut self.split_panes[self.split_focus];
                    pane.detail_target = Some(target);
                    pane.detail_scroll = 0;
                }
            }
            self.sync_detail_targets(poller_control);
        }
    }

    fn toggle_focus_or_view(&mut self) {
        match self.config.mode {
            Mode::Compact if !self.detail_open() => {
                self.compact_focus = self.compact_focus.toggle();
            }
            Mode::Split if self.split_panes[self.split_focus].detail_target.is_none() => {
                let pane = &mut self.split_panes[self.split_focus];
                pane.content = pane.content.toggle();
                self.sync_split_table_state(self.split_focus);
            }
            _ => {}
        }
    }

    fn switch_split_focus(&mut self, delta: i32) {
        if self.config.mode != Mode::Split {
            return;
        }
        let max = self
            .config
            .repos
            .len()
            .min(self.split_panes.len())
            .saturating_sub(1);
        self.split_focus = self
            .split_focus
            .saturating_add_signed(delta as isize)
            .min(max);
        self.sync_split_table_state(self.split_focus);
    }

    fn close_detail(&mut self, poller_control: &PollerControl) {
        match self.config.mode {
            Mode::Compact => {
                self.compact_detail_target = None;
                self.compact_detail_scroll = 0;
            }
            Mode::Split => {
                let pane = &mut self.split_panes[self.split_focus];
                pane.detail_target = None;
                pane.detail_scroll = 0;
            }
        }
        self.sync_detail_targets(poller_control);
    }

    fn anchor_selection(&mut self) {
        self.anchor_compact_selection();
        self.anchor_split_selection();
    }

    fn anchor_compact_selection(&mut self) {
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

    fn anchor_split_selection(&mut self) {
        for pane_index in 0..self.split_panes.len() {
            let Some(repo) = self.split_repo(pane_index) else {
                self.split_panes[pane_index].selected_action_id = None;
                self.split_panes[pane_index].selected_pr_id = None;
                self.split_panes[pane_index].actions_index = 0;
                self.split_panes[pane_index].prs_index = 0;
                self.split_panes[pane_index]
                    .table_state
                    .borrow_mut()
                    .select(None);
                continue;
            };
            let repo_slug = repo.slug();

            let visible_actions = self
                .state
                .actions
                .iter()
                .filter(|run| run.repo.slug() == repo_slug)
                .collect::<Vec<_>>();
            if !visible_actions.is_empty() {
                if let Some(selected_id) = self.split_panes[pane_index].selected_action_id {
                    if let Some(index) =
                        visible_actions.iter().position(|run| run.id == selected_id)
                    {
                        self.split_panes[pane_index].actions_index = index;
                    } else {
                        self.split_panes[pane_index].actions_index = self.split_panes[pane_index]
                            .actions_index
                            .min(visible_actions.len() - 1);
                        self.split_panes[pane_index].selected_action_id = visible_actions
                            .get(self.split_panes[pane_index].actions_index)
                            .map(|run| run.id);
                    }
                } else {
                    self.split_panes[pane_index].actions_index = 0;
                    self.split_panes[pane_index].selected_action_id =
                        visible_actions.first().map(|run| run.id);
                }
            } else {
                self.split_panes[pane_index].actions_index = 0;
                self.split_panes[pane_index].selected_action_id = None;
            }

            let visible_pulls = self
                .state
                .pulls
                .iter()
                .filter(|pr| pr.repo.slug() == repo_slug)
                .collect::<Vec<_>>();
            if !visible_pulls.is_empty() {
                if let Some(selected_id) = &self.split_panes[pane_index].selected_pr_id {
                    if let Some(index) = visible_pulls
                        .iter()
                        .position(|pr| pr.stable_id() == *selected_id)
                    {
                        self.split_panes[pane_index].prs_index = index;
                    } else {
                        self.split_panes[pane_index].prs_index = self.split_panes[pane_index]
                            .prs_index
                            .min(visible_pulls.len() - 1);
                        self.split_panes[pane_index].selected_pr_id = visible_pulls
                            .get(self.split_panes[pane_index].prs_index)
                            .map(|pr| pr.stable_id());
                    }
                } else {
                    self.split_panes[pane_index].prs_index = 0;
                    self.split_panes[pane_index].selected_pr_id =
                        visible_pulls.first().map(|pr| pr.stable_id());
                }
            } else {
                self.split_panes[pane_index].prs_index = 0;
                self.split_panes[pane_index].selected_pr_id = None;
            }

            self.sync_split_table_state(pane_index);
        }
    }

    fn sync_split_table_state(&self, pane_index: usize) {
        let selected = match self.split_panes[pane_index].content {
            FocusPane::Actions => {
                if self.split_actions(pane_index).is_empty() {
                    None
                } else {
                    Some(self.split_panes[pane_index].actions_index)
                }
            }
            FocusPane::PullRequests => {
                if self.split_pulls(pane_index).is_empty() {
                    None
                } else {
                    Some(self.split_panes[pane_index].prs_index)
                }
            }
        };
        self.split_panes[pane_index]
            .table_state
            .borrow_mut()
            .select(selected);
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
                    } else if app.current_detail_target().is_some() {
                        app.close_detail(&poller_control);
                    }
                }
                KeyCode::Left => app.switch_split_focus(-1),
                KeyCode::Right => app.switch_split_focus(1),
                KeyCode::Tab if !app.show_help => app.toggle_focus_or_view(),
                KeyCode::Char('r') => {
                    poller_control.request_refresh();
                }
                KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
                KeyCode::Char('g') => app.jump_to(false),
                KeyCode::Char('G') => app.jump_to(true),
                KeyCode::Enter if !app.show_help => app.open_detail(&poller_control),
                KeyCode::Char('l') | KeyCode::Char('o') if !app.show_help => {
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
