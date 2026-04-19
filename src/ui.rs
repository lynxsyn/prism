use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::{Frame, prelude::*};

use crate::app::App;
use crate::model::{
    DetailView, FocusPane, Mode, PullRequestCheckSummary, PullRequestDetail, PullRequestSummary,
    RateLimitState, WorkflowJobSummary, WorkflowRunDetail, WorkflowRunSummary,
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let size = frame.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)])
        .split(size);

    let body = outer[0];
    let status = outer[1];

    if body.width < minimum_width(app.config.mode) || body.height < minimum_height(app.config.mode)
    {
        let warning = Paragraph::new("Prism needs a larger terminal for this mode.")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::DOUBLE)
                    .title("Resize needed")
                    .padding(Padding::horizontal(2)),
            );
        frame.render_widget(warning, body);
        draw_status_bar(frame, status, app);
        return;
    }

    match app.config.mode {
        Mode::Split => draw_split(frame, body, app),
        Mode::Compact => draw_compact(frame, body, app),
    }

    draw_status_bar(frame, status, app);

    if app.show_help {
        draw_overlay(frame, overlay_rect(size, 72, 64), help_paragraph());
    }
}

fn draw_split(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    for pane_index in 0..2 {
        draw_split_pane(frame, sections[pane_index], app, pane_index);
    }
}

fn draw_split_pane(frame: &mut Frame<'_>, area: Rect, app: &App, pane_index: usize) {
    let Some(repo) = app.split_repo(pane_index) else {
        let block = pane_block("Empty pane".to_string(), false);
        let placeholder = Paragraph::new("No repo configured for this pane.").block(block);
        frame.render_widget(placeholder, area);
        return;
    };

    if app.split_detail_open(pane_index) {
        draw_detail_pane(
            frame,
            area,
            app,
            Some(pane_index),
            app.split_panes[pane_index].content,
            format!(
                "{}  ·  {} detail",
                repo.slug(),
                match app.split_panes[pane_index].content {
                    FocusPane::Actions => "Workflow",
                    FocusPane::PullRequests => "Pull request",
                }
            ),
            pane_index == app.split_focus,
        );
        return;
    }

    let title = format!(
        "{}  ·  {}",
        repo.slug(),
        view_label(app.split_panes[pane_index].content)
    );
    let mut table_state = app.split_panes[pane_index].table_state.borrow_mut();
    match app.split_panes[pane_index].content {
        FocusPane::Actions => draw_repo_actions_table(
            frame,
            area,
            app,
            &app.split_actions(pane_index),
            &title,
            pane_index == app.split_focus,
            &mut table_state,
        ),
        FocusPane::PullRequests => draw_repo_prs_table(
            frame,
            area,
            app,
            &app.split_pulls(pane_index),
            &title,
            pane_index == app.split_focus,
            &mut table_state,
        ),
    }
}

fn draw_compact(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    match (app.detail_open(), app.compact_focus) {
        (true, FocusPane::Actions) => draw_detail_pane(
            frame,
            sections[0],
            app,
            None,
            FocusPane::Actions,
            "Workflow detail".to_string(),
            true,
        ),
        _ => draw_actions_table(
            frame,
            sections[0],
            app,
            &mut app.actions_table_state.borrow_mut(),
        ),
    }

    match (app.detail_open(), app.compact_focus) {
        (true, FocusPane::PullRequests) => draw_detail_pane(
            frame,
            sections[1],
            app,
            None,
            FocusPane::PullRequests,
            "Pull request detail".to_string(),
            true,
        ),
        _ => draw_prs_table(
            frame,
            sections[1],
            app,
            &mut app.prs_table_state.borrow_mut(),
        ),
    }
}

fn draw_actions_table(frame: &mut Frame<'_>, area: Rect, app: &App, state: &mut TableState) {
    let header = Row::new([
        " Workflow ",
        " Repo ",
        " Branch ",
        " State ",
        " Age ",
        " Dur ",
    ])
    .style(header_style(app));
    let rows = if app.state.actions.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No workflow runs yet "),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        app.state
            .actions
            .iter()
            .map(|run| {
                Row::new(vec![
                    Cell::from(format!(" {} ", truncate(&run.workflow_name, 22))),
                    Cell::from(format!(" {} ", truncate(&run.repo.slug(), 18))),
                    Cell::from(format!(" {} ", truncate(&run.branch, 12))),
                    Cell::from(Text::styled(
                        format!(" {} ", format_run_state(run, app)),
                        state_style(
                            run.conclusion.as_deref(),
                            &run.status,
                            app.config.ui.no_color,
                        ),
                    )),
                    Cell::from(format!(" {} ", format_age(run.created_at))),
                    Cell::from(format!(" {} ", format_run_duration(run))),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(20),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(pane_block(
        "Actions".to_string(),
        app.compact_focus == FocusPane::Actions,
    ))
    .column_spacing(3)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_prs_table(frame: &mut Frame<'_>, area: Rect, app: &App, state: &mut TableState) {
    let header = Row::new([
        " Repo ",
        " # ",
        " Title ",
        " Author ",
        " Review ",
        " CI ",
        " Updated ",
    ])
    .style(header_style(app));
    let rows = if app.state.pulls.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No pull requests yet "),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        app.state
            .pulls
            .iter()
            .map(|pr| {
                let row_style = if pr.review_requested_for_viewer {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(format!(" {} ", truncate(&pr.repo.slug(), 18))),
                    Cell::from(format!(" #{} ", pr.number)),
                    Cell::from(format!(" {} ", truncate(&pr.title, 24))),
                    Cell::from(format!(" {} ", truncate(&pr.author, 10))),
                    Cell::from(format!(" {} ", review_state(pr))),
                    Cell::from(format!(" {} ", ci_state(pr))),
                    Cell::from(format!(" {} ", format_age(pr.updated_at))),
                ])
                .style(row_style)
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(6),
            Constraint::Min(18),
            Constraint::Length(12),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(pane_block(
        "Pull requests".to_string(),
        app.compact_focus == FocusPane::PullRequests,
    ))
    .column_spacing(3)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_repo_actions_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    rows_for_repo: &[&WorkflowRunSummary],
    title: &str,
    focused: bool,
    state: &mut TableState,
) {
    let header =
        Row::new([" Workflow ", " Branch ", " State ", " Age ", " Dur "]).style(header_style(app));
    let rows = if rows_for_repo.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No workflow runs yet "),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        rows_for_repo
            .iter()
            .map(|run| {
                Row::new(vec![
                    Cell::from(format!(" {} ", truncate(&run.workflow_name, 44))),
                    Cell::from(format!(" {} ", truncate(&run.branch, 24))),
                    Cell::from(Text::styled(
                        format!(" {} ", format_run_state(run, app)),
                        state_style(
                            run.conclusion.as_deref(),
                            &run.status,
                            app.config.ui.no_color,
                        ),
                    )),
                    Cell::from(format!(" {} ", format_age(run.created_at))),
                    Cell::from(format!(" {} ", format_run_duration(run))),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Min(40),
            Constraint::Length(26),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(pane_block(title.to_string(), focused))
    .column_spacing(3)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_repo_prs_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    rows_for_repo: &[&PullRequestSummary],
    title: &str,
    focused: bool,
    state: &mut TableState,
) {
    let header = Row::new([
        " # ",
        " Title ",
        " Author ",
        " Review ",
        " CI ",
        " Updated ",
    ])
    .style(header_style(app));
    let rows = if rows_for_repo.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(" No pull requests yet "),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        rows_for_repo
            .iter()
            .map(|pr| {
                let row_style = if pr.review_requested_for_viewer {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(format!(" #{} ", pr.number)),
                    Cell::from(format!(" {} ", truncate(&pr.title, 54))),
                    Cell::from(format!(" {} ", truncate(&pr.author, 16))),
                    Cell::from(format!(" {} ", review_state(pr))),
                    Cell::from(format!(" {} ", ci_state(pr))),
                    Cell::from(format!(" {} ", format_age(pr.updated_at))),
                ])
                .style(row_style)
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Min(48),
            Constraint::Length(18),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(pane_block(title.to_string(), focused))
    .column_spacing(3)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_detail_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    pane_index: Option<usize>,
    kind: FocusPane,
    title: String,
    focused: bool,
) {
    let lines = match kind {
        FocusPane::Actions => workflow_detail_lines(app, pane_index),
        FocusPane::PullRequests => pr_detail_lines(app, pane_index),
    };

    let paragraph = Paragraph::new(lines)
        .block(pane_block(title, focused))
        .scroll((app.detail_scroll_for(pane_index) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn workflow_detail_lines(app: &App, pane_index: Option<usize>) -> Vec<Line<'static>> {
    let detail = pane_index
        .and_then(|index| app.split_detail_target(index))
        .or_else(|| app.current_detail_target())
        .and_then(|target| match app.detail_view(target) {
            Some(DetailView::Workflow(detail)) => Some(detail),
            _ => None,
        });
    if let Some(detail) = detail {
        return workflow_detail_lines_loaded(app, detail);
    }

    let target = pane_index
        .and_then(|index| app.current_split_action(index))
        .or_else(|| app.current_action());
    let Some(target) = target else {
        return vec![Line::from("  No workflow run selected.  ")];
    };
    vec![
        Line::from(format!(
            "  {}  {}",
            target.repo.slug(),
            truncate(&target.workflow_name, 50)
        )),
        Line::from(""),
        Line::from("  Loading workflow detail...  "),
    ]
}

fn workflow_detail_lines_loaded(app: &App, detail: &WorkflowRunDetail) -> Vec<Line<'static>> {
    let ascii_only = app.config.ui.ascii_only;
    let run_style = state_style(
        detail.summary.conclusion.as_deref(),
        &detail.summary.status,
        app.config.ui.no_color,
    );
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "{}  ",
                    status_symbol_for_state(
                        &detail.summary.status,
                        detail.summary.conclusion.as_deref(),
                        ascii_only,
                        app.spinner_index,
                    )
                ),
                run_style,
            ),
            Span::styled(
                truncate(&detail.summary.workflow_name, 36),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(truncate(&detail.summary.title, 62)),
        ]),
        Line::from(""),
        kv_line("repo", detail.summary.repo.slug(), ascii_only),
        kv_line("branch", detail.summary.branch.clone(), ascii_only),
        kv_line("event", detail.summary.event.clone(), ascii_only),
        styled_kv_line(
            "state",
            format_run_state(&detail.summary, app),
            ascii_only,
            run_style,
        ),
        kv_line(
            "jobs",
            format!(
                "{} complete  ·  {} running  ·  {} failed",
                detail.completed_jobs, detail.running_jobs, detail.failed_jobs
            ),
            ascii_only,
        ),
        kv_line(
            "progress",
            progress_bar(detail.completed_jobs, detail.total_jobs, ascii_only, 18),
            ascii_only,
        ),
        section_line("job tree", detail.jobs.is_empty(), ascii_only),
    ];

    for (index, job) in detail.jobs.iter().enumerate() {
        lines.extend(detail_job_lines(
            job,
            index == detail.jobs.len().saturating_sub(1),
            ascii_only,
            app,
        ));
    }

    lines
}

fn pr_detail_lines(app: &App, pane_index: Option<usize>) -> Vec<Line<'static>> {
    let detail = pane_index
        .and_then(|index| app.split_detail_target(index))
        .or_else(|| app.current_detail_target())
        .and_then(|target| match app.detail_view(target) {
            Some(DetailView::PullRequest(detail)) => Some(detail),
            _ => None,
        });
    if let Some(detail) = detail {
        return pr_detail_lines_loaded(app, detail);
    }

    let target = pane_index
        .and_then(|index| app.current_split_pr(index))
        .or_else(|| app.current_pr());
    let Some(target) = target else {
        return vec![Line::from("  No pull request selected.  ")];
    };
    vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "{}  ",
                    status_symbol_for_pr_rollup(
                        target.ci_rollup.as_deref(),
                        app.config.ui.ascii_only,
                        app.spinner_index
                    )
                ),
                pr_rollup_style(target.ci_rollup.as_deref(), app.config.ui.no_color),
            ),
            Span::styled(
                format!("#{}", target.number),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(truncate(&target.title, 62)),
        ]),
        Line::from(""),
        Line::from("  Loading pull request detail...  "),
    ]
}

fn pr_detail_lines_loaded(app: &App, detail: &PullRequestDetail) -> Vec<Line<'static>> {
    let ascii_only = app.config.ui.ascii_only;
    let rollup_style = pr_rollup_style(detail.summary.ci_rollup.as_deref(), app.config.ui.no_color);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "{}  ",
                    status_symbol_for_pr_rollup(
                        detail.summary.ci_rollup.as_deref(),
                        ascii_only,
                        app.spinner_index,
                    )
                ),
                rollup_style,
            ),
            Span::styled(
                format!("#{}", detail.summary.number),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(truncate(&detail.summary.title, 42)),
        ]),
        Line::from(""),
        kv_line("repo", detail.summary.repo.slug(), ascii_only),
        kv_line("author", detail.summary.author.clone(), ascii_only),
        kv_line("review", review_state(&detail.summary), ascii_only),
        styled_kv_line(
            "checks",
            format!(
                "{} complete  ·  {} pass  ·  {} running  ·  {} pending  ·  {} failed",
                detail.completed_checks,
                detail.passing_checks,
                detail.running_checks,
                detail.pending_checks,
                detail.failing_checks
            ),
            ascii_only,
            rollup_style,
        ),
        kv_line(
            "progress",
            progress_bar(
                detail.completed_checks,
                detail.total_checks.max(1),
                ascii_only,
                18,
            ),
            ascii_only,
        ),
        section_line("check tree", detail.checks.is_empty(), ascii_only),
    ];

    if detail.checks.is_empty() {
        lines.push(Line::from(format!(
            " {} no checks reported yet",
            nested_branch(true, ascii_only)
        )));
        return lines;
    }

    for (index, check) in detail.checks.iter().enumerate() {
        lines.extend(detail_pr_check_lines(
            check,
            index == detail.checks.len().saturating_sub(1),
            ascii_only,
            app,
        ));
    }

    lines
}

fn kv_line(label: &str, value: String, ascii_only: bool) -> Line<'static> {
    Line::from(format!(
        " {} {:<9} {}",
        tree_branch(false, ascii_only),
        label,
        value
    ))
}

fn styled_kv_line(label: &str, value: String, ascii_only: bool, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!(" {} {:<9} ", tree_branch(false, ascii_only), label)),
        Span::styled(value, style),
    ])
}

fn section_line(label: &str, last: bool, ascii_only: bool) -> Line<'static> {
    Line::from(format!(" {} {}", tree_branch(last, ascii_only), label))
}

fn detail_job_lines(
    job: &WorkflowJobSummary,
    last_job: bool,
    ascii_only: bool,
    app: &App,
) -> Vec<Line<'static>> {
    let job_style = state_style(
        job.conclusion.as_deref(),
        &job.status,
        app.config.ui.no_color,
    );
    let job_progress = if job.indeterminate_progress {
        status_meter("IN_PROGRESS", None, ascii_only, app.spinner_index, 10)
    } else if job.total_steps > 0 {
        progress_bar(job.completed_steps, job.total_steps, ascii_only, 10)
    } else {
        "-".to_string()
    };

    let mut lines = vec![Line::from(vec![
        Span::raw(format!(" {} ", nested_branch(last_job, ascii_only))),
        Span::styled(
            format!(
                "{} ",
                status_symbol_for_state(
                    &job.status,
                    job.conclusion.as_deref(),
                    ascii_only,
                    app.spinner_index,
                )
            ),
            job_style,
        ),
        Span::styled(
            truncate(&job.name, 24),
            job_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(job_progress, job_style),
        Span::raw("  "),
        Span::styled(
            format!(
                "[{}]",
                detail_state_badge(&job.status, job.conclusion.as_deref())
            ),
            job_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format_duration(job.started_at, job.completed_at),
            Style::default().add_modifier(Modifier::DIM),
        ),
    ])];

    lines.push(Line::from(vec![
        Span::raw(format!(
            " {} {} state        ",
            nested_child_prefix(last_job, ascii_only),
            tree_branch(job.failed_step_name.is_none(), ascii_only)
        )),
        Span::styled(format_job_state(job, app), job_style),
    ]));

    if let Some(step) = &job.failed_step_name {
        let failed_marker = if ascii_only { "x" } else { "✕" };
        lines.push(Line::from(vec![
            Span::raw(format!(
                " {} {} failed step  ",
                nested_child_prefix(last_job, ascii_only),
                tree_branch(true, ascii_only)
            )),
            Span::styled(
                format!("{failed_marker} {step}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    lines
}

fn detail_pr_check_lines(
    check: &PullRequestCheckSummary,
    last_check: bool,
    ascii_only: bool,
    app: &App,
) -> Vec<Line<'static>> {
    let check_style = pr_check_style(check, app.config.ui.no_color);
    let label = match &check.workflow_name {
        Some(workflow_name) if workflow_name != &check.name => {
            format!(
                "{}  /  {}",
                truncate(workflow_name, 16),
                truncate(&check.name, 18)
            )
        }
        _ => truncate(&check.name, 32),
    };
    let meter = status_meter(
        &check.status,
        check.conclusion.as_deref(),
        ascii_only,
        app.spinner_index,
        10,
    );

    let mut lines = vec![Line::from(vec![
        Span::raw(format!(" {} ", nested_branch(last_check, ascii_only))),
        Span::styled(
            format!(
                "{} ",
                status_symbol_for_state(
                    &check.status,
                    check.conclusion.as_deref(),
                    ascii_only,
                    app.spinner_index,
                )
            ),
            check_style,
        ),
        Span::styled(label, check_style.add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(meter, check_style),
        Span::raw("  "),
        Span::styled(
            format!(
                "[{}] {}",
                detail_state_badge(&check.status, check.conclusion.as_deref()),
                check_status_label(check, app)
            ),
            check_style.add_modifier(Modifier::BOLD),
        ),
    ])];

    lines.push(Line::from(vec![
        Span::raw(format!(
            " {} {} timing       ",
            nested_child_prefix(last_check, ascii_only),
            tree_branch(check.url.is_none(), ascii_only)
        )),
        Span::styled(
            format_duration(check.started_at, check.completed_at),
            check_style,
        ),
    ]));

    if let Some(url) = &check.url {
        lines.push(Line::from(vec![
            Span::raw(format!(
                " {} {} link         ",
                nested_child_prefix(last_check, ascii_only),
                tree_branch(true, ascii_only)
            )),
            Span::raw(truncate(url, 44)),
        ]));
    }

    lines
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, widget: impl Widget) {
    frame.render_widget(Clear, area);
    frame.render_widget(widget, area);
}

fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let now = Utc::now();
    let next_countdown = app
        .state
        .next_refresh_at
        .map(|next| (next - now).num_seconds().max(0))
        .map(|secs| format!("{secs}s"))
        .unwrap_or_else(|| "-".to_string());

    let rate = app
        .state
        .rate_limit
        .as_ref()
        .map(rate_limit_label)
        .unwrap_or_else(|| "rate:-".to_string());
    let refreshed = app
        .state
        .last_refresh_at
        .map(|ts| ts.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_string());

    let focus = match app.config.mode {
        Mode::Compact => format!("focus:{}", view_label(app.compact_focus)),
        Mode::Split => format!(
            "focus:{}/{}",
            app.current_repo_label().unwrap_or_else(|| "-".to_string()),
            if app.split_detail_open(app.split_focus) {
                "detail".to_string()
            } else {
                view_label(app.focused_view()).to_string()
            }
        ),
    };
    let stale = if app.state.errors.is_empty() {
        ""
    } else {
        " | stale"
    };
    let controls = match app.config.mode {
        Mode::Compact => {
            if app.detail_open() {
                " | esc:back | l:link"
            } else {
                " | enter:detail | l:link"
            }
        }
        Mode::Split => {
            if app.split_detail_open(app.split_focus) {
                " | ←→:repo | esc:back | l:link"
            } else {
                " | ←→:repo | tab:view | enter:detail | l:link"
            }
        }
    };

    let text = format!(
        " mode:{} | {} | refresh:{} | next:{} | {} | host:{}{}{}",
        app.config.mode, focus, refreshed, next_countdown, rate, app.config.host, stale, controls
    );
    let widget = Paragraph::new(text).style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(widget, area);
}

fn help_paragraph() -> Paragraph<'static> {
    let lines = vec![
        Line::from("Prism keybindings"),
        Line::from(""),
        Line::from(" q         quit"),
        Line::from(" r         force refresh"),
        Line::from(" Tab       toggle Actions / Pull requests in the focused pane"),
        Line::from(" ← / →     move focus between repo panes in split mode"),
        Line::from(" j / ↓     move down"),
        Line::from(" k / ↑     move up"),
        Line::from(" g / G     jump to top / bottom"),
        Line::from(" Enter     open selected item detail in the current pane"),
        Line::from(" l / o     open the selected item in the browser"),
        Line::from(" Esc       close help or return from detail"),
        Line::from(" ?         toggle help"),
    ];
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::DOUBLE)
                .title("Help")
                .padding(Padding::horizontal(2)),
        )
        .wrap(Wrap { trim: false })
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let title = if focused {
        format!("{title}  •")
    } else {
        title
    };
    let style = if focused {
        Style::default()
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_set(border::DOUBLE)
        .border_style(style)
        .padding(Padding::horizontal(2))
        .title(title)
}

fn header_style(app: &App) -> Style {
    if app.config.ui.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }
}

fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn state_style(conclusion: Option<&str>, status: &str, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match (status, conclusion) {
        ("completed" | "COMPLETED", Some("success" | "SUCCESS")) => {
            Style::default().fg(Color::Green)
        }
        ("completed" | "COMPLETED", Some("skipped" | "SKIPPED" | "NEUTRAL")) => {
            Style::default().fg(Color::DarkGray)
        }
        (
            "completed" | "COMPLETED",
            Some(
                "failure" | "FAILURE" | "ERROR" | "timed_out" | "TIMED_OUT" | "cancelled"
                | "CANCELLED" | "ACTION_REQUIRED",
            ),
        ) => Style::default().fg(Color::Red),
        ("completed" | "COMPLETED", _) => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Blue),
    }
}

fn pr_rollup_style(rollup: Option<&str>, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match rollup {
        Some("SUCCESS") => Style::default().fg(Color::Green),
        Some("FAILURE" | "ERROR") => Style::default().fg(Color::Red),
        Some("PENDING" | "EXPECTED") => Style::default().fg(Color::Blue),
        Some("SKIPPED") => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Yellow),
    }
}

fn pr_check_style(check: &PullRequestCheckSummary, no_color: bool) -> Style {
    state_style(check.conclusion.as_deref(), &check.status, no_color)
}

fn status_symbol_for_state(
    status: &str,
    conclusion: Option<&str>,
    ascii_only: bool,
    spinner_index: usize,
) -> String {
    match (status, conclusion) {
        ("completed" | "COMPLETED", Some("success" | "SUCCESS")) => {
            if ascii_only {
                "+".to_string()
            } else {
                "✓".to_string()
            }
        }
        ("completed" | "COMPLETED", Some("skipped" | "SKIPPED" | "NEUTRAL")) => "-".to_string(),
        (
            "completed" | "COMPLETED",
            Some(
                "failure" | "FAILURE" | "ERROR" | "cancelled" | "CANCELLED" | "timed_out"
                | "TIMED_OUT" | "ACTION_REQUIRED",
            ),
        ) => {
            if ascii_only {
                "x".to_string()
            } else {
                "✕".to_string()
            }
        }
        ("queued" | "QUEUED" | "PENDING" | "EXPECTED" | "WAITING" | "REQUESTED", _) => {
            if ascii_only {
                ".".to_string()
            } else {
                "·".to_string()
            }
        }
        _ => spinner_frame(spinner_index, ascii_only),
    }
}

fn status_symbol_for_pr_rollup(
    rollup: Option<&str>,
    ascii_only: bool,
    spinner_index: usize,
) -> String {
    match rollup {
        Some("SUCCESS") => {
            if ascii_only {
                "+".to_string()
            } else {
                "✓".to_string()
            }
        }
        Some("FAILURE" | "ERROR") => {
            if ascii_only {
                "x".to_string()
            } else {
                "✕".to_string()
            }
        }
        Some("PENDING" | "EXPECTED") => spinner_frame(spinner_index, ascii_only),
        Some("SKIPPED") => "-".to_string(),
        _ => {
            if ascii_only {
                "?".to_string()
            } else {
                "·".to_string()
            }
        }
    }
}

fn status_meter(
    status: &str,
    conclusion: Option<&str>,
    ascii_only: bool,
    spinner_index: usize,
    width: usize,
) -> String {
    let filled = match status {
        "completed" | "COMPLETED" => width,
        "IN_PROGRESS" | "RUNNING" | "in_progress" => {
            let min_fill = (width / 3).max(1);
            min_fill + (spinner_index % (width.saturating_sub(min_fill).max(1)))
        }
        "queued" | "QUEUED" | "PENDING" | "EXPECTED" | "WAITING" | "REQUESTED" => 0,
        _ if conclusion.is_some() => width,
        _ => (width / 2).max(1),
    };
    let fill = filled.min(width);
    let full = if ascii_only { '#' } else { '█' };
    let empty = if ascii_only { '-' } else { '░' };
    format!(
        "[{}{}]",
        full.to_string().repeat(fill),
        empty.to_string().repeat(width.saturating_sub(fill))
    )
}

fn check_status_label(check: &PullRequestCheckSummary, app: &App) -> String {
    match (check.status.as_str(), check.conclusion.as_deref()) {
        ("COMPLETED", Some("SUCCESS")) => "pass".to_string(),
        ("COMPLETED", Some("SKIPPED" | "NEUTRAL")) => "skipped".to_string(),
        ("COMPLETED", Some("FAILURE" | "ERROR")) => "fail".to_string(),
        ("COMPLETED", Some("CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED")) => "stopped".to_string(),
        ("IN_PROGRESS" | "RUNNING", _) => format!(
            "{} running",
            spinner_frame(app.spinner_index, app.config.ui.ascii_only)
        ),
        ("QUEUED" | "PENDING" | "EXPECTED" | "WAITING" | "REQUESTED", _) => "pending".to_string(),
        _ => check.status.to_ascii_lowercase(),
    }
}

fn detail_state_badge(status: &str, conclusion: Option<&str>) -> &'static str {
    match (status, conclusion) {
        ("completed" | "COMPLETED", Some("success" | "SUCCESS")) => "PASS",
        ("completed" | "COMPLETED", Some("skipped" | "SKIPPED" | "NEUTRAL")) => "SKIP",
        (
            "completed" | "COMPLETED",
            Some(
                "failure" | "FAILURE" | "ERROR" | "cancelled" | "CANCELLED" | "timed_out"
                | "TIMED_OUT" | "ACTION_REQUIRED",
            ),
        ) => "FAIL",
        ("queued" | "QUEUED" | "PENDING" | "EXPECTED" | "WAITING" | "REQUESTED", _) => "WAIT",
        ("IN_PROGRESS" | "RUNNING" | "in_progress", _) => "RUN ",
        ("completed" | "COMPLETED", _) => "DONE",
        _ => "INFO",
    }
}

fn format_run_state(run: &WorkflowRunSummary, app: &App) -> String {
    let spinner = spinner_frame(app.spinner_index, app.config.ui.ascii_only);
    let queued = if app.config.ui.ascii_only { "." } else { "·" };
    let success = if app.config.ui.ascii_only { "+" } else { "✓" };
    let fail = if app.config.ui.ascii_only { "x" } else { "✕" };
    let skip = "-";

    match (run.status.as_str(), run.conclusion.as_deref()) {
        ("completed", Some("success")) => format!("{success} success"),
        ("completed", Some("skipped")) => format!("{skip} skipped"),
        ("completed", Some("failure")) => format!("{fail} failure"),
        ("completed", Some("cancelled")) => format!("{fail} cancelled"),
        ("completed", Some("timed_out")) => format!("{fail} timeout"),
        ("queued", _) => format!("{queued} queued"),
        _ => format!("{spinner} running"),
    }
}

fn format_job_state(job: &WorkflowJobSummary, app: &App) -> String {
    if job.status == "completed" {
        match job.conclusion.as_deref() {
            Some("success") => "[PASS] success".to_string(),
            Some("failure") => "[FAIL] failure".to_string(),
            Some("cancelled") => "[FAIL] cancelled".to_string(),
            Some("timed_out") => "[FAIL] timeout".to_string(),
            _ => "[DONE] complete".to_string(),
        }
    } else {
        format!(
            "[RUN ] {} running",
            spinner_frame(app.spinner_index, app.config.ui.ascii_only)
        )
    }
}

fn review_state(pr: &PullRequestSummary) -> String {
    if pr.is_draft {
        return "draft".to_string();
    }

    match pr.review_decision.as_deref() {
        Some("APPROVED") => "approved".to_string(),
        Some("CHANGES_REQUESTED") => "changes".to_string(),
        Some("REVIEW_REQUIRED") => "review".to_string(),
        _ if pr.review_requested_for_viewer => "requested".to_string(),
        _ => "open".to_string(),
    }
}

fn ci_state(pr: &PullRequestSummary) -> String {
    match pr.ci_rollup.as_deref() {
        Some("SUCCESS") => "pass".to_string(),
        Some("FAILURE" | "ERROR") => "fail".to_string(),
        Some("PENDING" | "EXPECTED") => "pending".to_string(),
        Some("SKIPPED") => "skipped".to_string(),
        _ => "-".to_string(),
    }
}

fn format_run_duration(run: &WorkflowRunSummary) -> String {
    format_duration(
        run.started_at,
        run.updated_at.filter(|_| run.status == "completed"),
    )
}

pub fn format_duration(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> String {
    let Some(start) = start else {
        return "-".to_string();
    };
    let end = end.unwrap_or_else(Utc::now);
    let secs = (end - start).num_seconds().max(0);
    format_compact_duration(secs)
}

pub fn format_age(timestamp: DateTime<Utc>) -> String {
    let secs = (Utc::now() - timestamp).num_seconds().max(0);
    format_compact_duration(secs)
}

fn format_compact_duration(total_secs: i64) -> String {
    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3_600 {
        format!("{}m", total_secs / 60)
    } else {
        format!("{}h", total_secs / 3_600)
    }
}

fn progress_bar(done: usize, total: usize, ascii_only: bool, width: usize) -> String {
    if total == 0 {
        return "-".to_string();
    }
    let filled = ((done as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let full = if ascii_only { '#' } else { '█' };
    let empty = if ascii_only { '-' } else { '░' };
    format!(
        "[{}{}] {done}/{total}",
        full.to_string().repeat(filled),
        empty.to_string().repeat(width.saturating_sub(filled))
    )
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn minimum_width(mode: Mode) -> u16 {
    match mode {
        Mode::Compact => 56,
        Mode::Split => 60,
    }
}

fn minimum_height(mode: Mode) -> u16 {
    match mode {
        Mode::Compact => 12,
        Mode::Split => 18,
    }
}

fn overlay_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn rate_limit_label(rate_limit: &RateLimitState) -> String {
    let mut label = format!("rate:{}/{}", rate_limit.remaining, rate_limit.limit);
    if let Some(retry_after) = rate_limit.retry_after {
        label.push_str(&format!(" wait:{retry_after}s"));
    } else if rate_limit.remaining <= 100 {
        if let Some(reset_at) = rate_limit.reset_at {
            label.push_str(&format!(" reset:{}", reset_at.format("%H:%M")));
        }
    } else if rate_limit.used > 0 {
        label.push_str(&format!(" used:{}", rate_limit.used));
    }
    label
}

fn view_label(kind: FocusPane) -> &'static str {
    match kind {
        FocusPane::Actions => "Actions",
        FocusPane::PullRequests => "Pull requests",
    }
}

fn tree_branch(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "\\-",
        (false, true) => "+-",
        (true, false) => "└─",
        (false, false) => "├─",
    }
}

fn nested_branch(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "   \\-",
        (false, true) => "   +-",
        (true, false) => "   └─",
        (false, false) => "   ├─",
    }
}

fn nested_child_prefix(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "      ",
        (false, true) => "   |  ",
        (true, false) => "      ",
        (false, false) => "   │  ",
    }
}

fn spinner_frame(index: usize, ascii_only: bool) -> String {
    let frames = if ascii_only {
        &["-", "\\", "|", "/"][..]
    } else {
        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][..]
    };
    frames[index % frames.len()].to_string()
}
