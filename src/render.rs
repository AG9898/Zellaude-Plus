use crate::state::{
    unix_now, unix_now_ms, Activity, AgentSource, ClickRegion, FlashMode, MenuAction,
    MenuClickRegion, NotifyMode, SessionInfo, SettingKey, State, ViewMode,
};
use std::fmt::Write;
use std::io::Write as IoWrite;
use zellij_tile::prelude::{InputMode, TabInfo};

struct Style {
    symbol: &'static str,
    r: u8,
    g: u8,
    b: u8,
}

fn activity_priority(activity: &Activity) -> u8 {
    match activity {
        Activity::Waiting => 8,
        Activity::Tool(_) => 7,
        Activity::Thinking => 6,
        Activity::Prompting => 5,
        Activity::Notification => 4,
        Activity::Init => 3,
        Activity::Done => 2,
        Activity::AgentDone => 1,
        Activity::Idle => 0,
    }
}

fn tool_symbol(name: &str) -> &'static str {
    match name {
        "Bash" | "shell"           => "⚡",
        "Read" | "Glob" | "Grep"   => "◉",
        "Edit" | "Write"           => "✎",
        "Task"                     => "⊜",
        "WebSearch" | "WebFetch"   => "◈",
        _                          => "⚙",
    }
}

// Claude: warm orange/amber palette — brand color ~(210, 105, 58)
fn activity_style_claude(activity: &Activity) -> Style {
    match activity {
        Activity::Init         => Style { symbol: "◆", r: 175, g: 165, b: 155 },
        Activity::Thinking     => Style { symbol: "●", r: 215, g: 145, b: 95  },
        Activity::Tool(name)   => Style { symbol: tool_symbol(name), r: 240, g: 150, b: 60  },
        Activity::Prompting    => Style { symbol: "▶", r: 100, g: 210, b: 140 },
        Activity::Waiting      => Style { symbol: "⚠", r: 255, g: 60,  b: 60  },
        Activity::Notification => Style { symbol: "◇", r: 220, g: 180, b: 110 },
        Activity::Done         => Style { symbol: "✓", r: 100, g: 210, b: 140 },
        Activity::AgentDone    => Style { symbol: "✓", r: 80,  g: 195, b: 120 },
        Activity::Idle         => Style { symbol: "○", r: 175, g: 165, b: 155 },
    }
}

// Codex / OpenAI: teal-green palette — brand color #10A37F = (16, 163, 127)
fn activity_style_codex(activity: &Activity) -> Style {
    match activity {
        Activity::Init         => Style { symbol: "◆", r: 155, g: 175, b: 170 },
        Activity::Thinking     => Style { symbol: "●", r: 60,  g: 175, b: 158 },
        Activity::Tool(name)   => Style { symbol: tool_symbol(name), r: 50,  g: 195, b: 168 },
        Activity::Prompting    => Style { symbol: "▶", r: 16,  g: 163, b: 127 },
        Activity::Waiting      => Style { symbol: "⚠", r: 255, g: 60,  b: 60  },
        Activity::Notification => Style { symbol: "◇", r: 100, g: 200, b: 180 },
        Activity::Done         => Style { symbol: "✓", r: 16,  g: 163, b: 127 },
        Activity::AgentDone    => Style { symbol: "✓", r: 20,  g: 150, b: 118 },
        Activity::Idle         => Style { symbol: "○", r: 155, g: 175, b: 170 },
    }
}

fn activity_style(activity: &Activity, source: AgentSource) -> Style {
    match source {
        AgentSource::Claude => activity_style_claude(activity),
        AgentSource::Codex  => activity_style_codex(activity),
    }
}

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ELAPSED_THRESHOLD: u64 = 30;
const SEPARATOR: &str = "\u{e0b0}";

type Color = (u8, u8, u8);
const BAR_BG: Color = (30, 30, 46);
const PREFIX_BG: Color = (60, 50, 80);
const PREFIX_BG_SETTINGS: Color = (100, 70, 140);
const TAB_BG_ACTIVE: Color = (140, 100, 200);
const TAB_BG_INACTIVE: Color = (80, 75, 110);
const FLASH_BG_BRIGHT: Color = (80, 80, 30);

/// Write a powerline arrow: fg=from_bg, bg=to_bg, then separator char.
fn arrow(buf: &mut String, col: &mut usize, from: Color, to: Color) {
    let _ = write!(
        buf,
        "{}{}{SEPARATOR}",
        fg(from.0, from.1, from.2),
        bg(to.0, to.1, to.2),
    );
    *col += 1;
}

fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn mode_style(mode: InputMode) -> (Color, &'static str) {
    match mode {
        InputMode::Normal => ((80, 200, 120), "NORMAL"),
        InputMode::Locked => ((255, 80, 80), "LOCKED"),
        InputMode::Pane => ((80, 180, 255), "PANE"),
        InputMode::Tab => ((180, 140, 255), "TAB"),
        InputMode::Resize => ((255, 170, 50), "RESIZE"),
        InputMode::Move => ((255, 170, 50), "MOVE"),
        InputMode::Scroll => ((200, 200, 100), "SCROLL"),
        InputMode::EnterSearch => ((200, 200, 100), "SEARCH"),
        InputMode::Search => ((200, 200, 100), "SEARCH"),
        InputMode::RenameTab => ((200, 200, 100), "RENAME"),
        InputMode::RenamePane => ((200, 200, 100), "RENAME"),
        InputMode::Session => ((180, 140, 255), "SESSION"),
        InputMode::Prompt => ((80, 200, 120), "PROMPT"),
        InputMode::Tmux => ((80, 200, 120), "TMUX"),
    }
}

pub fn render_status_bar(state: &mut State, _rows: usize, cols: usize) {
    state.click_regions.clear();
    state.menu_click_regions.clear();

    let mut buf = String::with_capacity(cols * 4);
    // Terminal setup for a 1-row status bar:
    //  \x1b[H     — cursor home (prevent scroll from cursor at end-of-line)
    //  \x1b[?7l   — disable auto-wrap (clip overflow instead of scroll)
    //  \x1b[?25l  — hide cursor
    buf.push_str("\x1b[H\x1b[?7l\x1b[?25l");
    let bar_bg_str = bg(BAR_BG.0, BAR_BG.1, BAR_BG.2);

    // Bail early if terminal is too narrow
    if cols < 5 {
        let _ = write!(buf, "{bar_bg_str}{:width$}{RESET}", "", width = cols);
        print!("{buf}");
        let _ = std::io::stdout().flush();
        return;
    }

    let prefix_bg = if state.view_mode == ViewMode::Settings {
        PREFIX_BG_SETTINGS
    } else {
        PREFIX_BG
    };

    // Build prefix: " Zellaude (session) MODE "
    let (mode_bg, mode_text) = mode_style(state.input_mode);
    let show_mode = state.settings.mode_indicator;
    let session_part = match state.zellij_session_name.as_deref() {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    let prefix_text = format!(" Zellaude{session_part} ");
    let prefix_width = display_width(&prefix_text);
    let mode_pill_width = if show_mode { 1 + mode_text.len() + 1 } else { 0 };
    let total_prefix_width = prefix_width + mode_pill_width;

    // Render prefix segment (truncate if wider than cols)
    let mut col;
    if total_prefix_width <= cols {
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        if show_mode {
            let _ = write!(
                buf,
                "{}{}{BOLD} {mode_text} {RESET}",
                bg(mode_bg.0, mode_bg.1, mode_bg.2),
                fg(30, 30, 46),
            );
        }
        col = total_prefix_width;
    } else if prefix_width <= cols {
        // Fit the name part but skip mode pill
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        col = prefix_width;
    } else {
        // Even name doesn't fit — just show what we can
        let avail = cols.saturating_sub(2); // leave room for fill
        let short: String = prefix_text.chars().take(avail).collect();
        let _ = write!(
            buf,
            "{}{}{BOLD}{short}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        col = display_width(&short);
    }
    state.prefix_click_region = Some((0, col));

    let last_prefix_bg = if show_mode && total_prefix_width <= cols { mode_bg } else { prefix_bg };
    let prefix_used = col;

    if col < cols {
        match state.view_mode {
            ViewMode::Normal => {
                render_tabs(state, &mut buf, &mut col, cols, last_prefix_bg, prefix_used);
            }
            ViewMode::Settings => {
                arrow(&mut buf, &mut col, last_prefix_bg, BAR_BG);
                let _ = write!(buf, "{bar_bg_str}");
                render_settings_menu(state, &mut buf, &mut col);
            }
        }
    }

    // Fill remaining width with bar background — never exceed cols
    if col < cols {
        let remaining = cols - col;
        let _ = write!(buf, "{bar_bg_str}{:width$}", "", width = remaining);
    }
    let _ = write!(buf, "{RESET}");

    print!("{buf}");
    let _ = std::io::stdout().flush();
}

fn render_tabs(
    state: &mut State,
    buf: &mut String,
    col: &mut usize,
    cols: usize,
    prefix_bg: Color,
    prefix_width: usize,
) {
    let now_s = unix_now();
    let now_ms = unix_now_ms();

    let mut tabs: Vec<&TabInfo> = state.tabs.iter().collect();
    tabs.sort_by_key(|t| t.position);

    let count = tabs.len();
    if count == 0 {
        arrow(buf, col, prefix_bg, BAR_BG);
        return;
    }

    // For each tab, find the best session per source independently
    let tab_sessions: Vec<(Option<&SessionInfo>, Option<&SessionInfo>)> = tabs
        .iter()
        .map(|tab| {
            let best_claude = state.sessions.values()
                .filter(|s| s.tab_index == Some(tab.position) && s.source == AgentSource::Claude)
                .max_by_key(|s| activity_priority(&s.activity));
            let best_codex = state.sessions.values()
                .filter(|s| s.tab_index == Some(tab.position) && s.source == AgentSource::Codex)
                .max_by_key(|s| activity_priority(&s.activity));
            (best_claude, best_codex)
        })
        .collect();

    // Elapsed: use the highest-priority session across both sources
    let elapsed_strs: Vec<Option<String>> = tab_sessions
        .iter()
        .map(|(claude, codex)| {
            if !state.settings.elapsed_time {
                return None;
            }
            let best = match (claude, codex) {
                (Some(c), Some(x)) => {
                    if activity_priority(&c.activity) >= activity_priority(&x.activity) {
                        Some(*c)
                    } else {
                        Some(*x)
                    }
                }
                (Some(c), None) => Some(*c),
                (None, Some(x)) => Some(*x),
                (None, None) => None,
            };
            best.and_then(|s| {
                let elapsed = now_s.saturating_sub(s.last_event_ts);
                if elapsed >= ELAPSED_THRESHOLD {
                    Some(format_elapsed(elapsed))
                } else {
                    None
                }
            })
        })
        .collect();

    // Overhead: 2 base (leading + trailing) + 2 per tracked symbol (symbol + space-or-gap)
    let total_elapsed_width: usize = elapsed_strs
        .iter()
        .map(|e| e.as_ref().map_or(0, |s| s.len() + 1))
        .sum();
    let per_tab_overhead: usize = tab_sessions
        .iter()
        .map(|(c, x)| {
            let n = c.is_some() as usize + x.is_some() as usize;
            2 + n * 2
        })
        .sum();
    let overhead = prefix_width + 2 * count + per_tab_overhead + total_elapsed_width;
    let max_name_len = if overhead < cols {
        ((cols - overhead) / count).min(20)
    } else {
        0
    };

    let mut prev_bg = prefix_bg;

    for (i, tab) in tabs.iter().enumerate() {
        let arrows_needed = if prev_bg == prefix_bg { 1 } else { 2 };
        if *col + arrows_needed + 3 > cols {
            break;
        }

        let (claude_session, codex_session) = tab_sessions[i];
        let is_tracked = claude_session.is_some() || codex_session.is_some();
        let tab_name = &tab.name;

        let char_count = tab_name.chars().count();
        let truncated = if max_name_len == 0 {
            String::new()
        } else if char_count > max_name_len {
            let s: String = tab_name.chars().take(max_name_len.saturating_sub(1)).collect();
            format!("{s}…")
        } else {
            tab_name.to_string()
        };

        let is_flash_bright = state.sessions.values()
            .filter(|s| s.tab_index == Some(tab.position))
            .any(|s| {
                state.flash_deadlines
                    .get(&s.pane_id)
                    .map(|&deadline| now_ms < deadline && (now_ms / 250) % 2 == 0)
                    .unwrap_or(false)
            });

        let is_active = tab.active;
        let tab_bg = if is_flash_bright { FLASH_BG_BRIGHT }
                     else if is_active  { TAB_BG_ACTIVE }
                     else               { TAB_BG_INACTIVE };

        if prev_bg == prefix_bg {
            arrow(buf, col, prev_bg, tab_bg);
        } else {
            arrow(buf, col, prev_bg, BAR_BG);
            arrow(buf, col, BAR_BG, tab_bg);
        }

        let tab_bg_str = bg(tab_bg.0, tab_bg.1, tab_bg.2);
        let region_start = *col;

        if is_tracked {
            // Winning session determines name styling
            let winning = match (claude_session, codex_session) {
                (Some(c), Some(x)) => {
                    if activity_priority(&c.activity) >= activity_priority(&x.activity) { c } else { x }
                }
                (Some(c), None) => c,
                (None, Some(x)) => x,
                (None, None) => unreachable!(),
            };

            let (name_fg, name_bold) = if is_flash_bright {
                (fg(255, 255, 80), true)
            } else if is_active {
                (fg(255, 255, 255), true)
            } else {
                let inactive = match winning.source {
                    AgentSource::Claude => fg(235, 195, 165), // warm amber
                    AgentSource::Codex  => fg(120, 205, 195), // cool teal
                };
                (inactive, false)
            };

            // Leading space
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            // Claude symbol
            if let Some(s) = claude_session {
                let style = activity_style(&s.activity, AgentSource::Claude);
                let sym_fg = if is_flash_bright { fg(255, 255, 80) } else { fg(style.r, style.g, style.b) };
                let _ = write!(buf, "{sym_fg}{}", style.symbol);
                *col += display_width(style.symbol);
            }

            // Gap between symbols when both present
            if claude_session.is_some() && codex_session.is_some() {
                let _ = write!(buf, "{tab_bg_str} ");
                *col += 1;
            }

            // Codex symbol
            if let Some(s) = codex_session {
                let style = activity_style(&s.activity, AgentSource::Codex);
                let sym_fg = if is_flash_bright { fg(255, 255, 80) } else { fg(style.r, style.g, style.b) };
                let _ = write!(buf, "{sym_fg}{}", style.symbol);
                *col += display_width(style.symbol);
            }

            // Space + name
            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, " {bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += 1 + display_width(&truncated);
            }

            // Elapsed
            if let Some(ref es) = elapsed_strs[i] {
                if *col + 1 + es.len() + 1 < cols {
                    let _ = write!(buf, " {}{es}", fg(165, 160, 180));
                    *col += 1 + es.len();
                }
            }

            // Fullscreen indicator
            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            // Trailing space
            let _ = write!(buf, " ");
            *col += 1;

            let waiting_session = state.sessions.values()
                .filter(|s| s.tab_index == Some(tab.position))
                .find(|s| matches!(s.activity, Activity::Waiting));

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: waiting_session.map_or(0, |s| s.pane_id),
                is_waiting: waiting_session.is_some(),
            });
        } else {
            // Untracked tab — no symbol, dimmer name
            let name_fg   = if is_active { fg(220, 215, 230) } else { fg(170, 165, 185) };
            let name_bold = is_active;

            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, "{bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += display_width(&truncated);
            }

            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            let _ = write!(buf, " ");
            *col += 1;

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: 0,
                is_waiting: false,
            });
        }

        prev_bg = tab_bg;
    }

    if prev_bg != prefix_bg || count > 0 {
        arrow(buf, col, prev_bg, BAR_BG);
    }
}

fn notify_mode_label(mode: NotifyMode) -> (&'static str, &'static str, String, String) {
    match mode {
        NotifyMode::Always => ("●", "Notify: always", fg(80, 200, 120), fg(255, 255, 255)),
        NotifyMode::Unfocused => ("◐", "Notify: unfocused", fg(255, 200, 60), fg(255, 200, 60)),
        NotifyMode::Never => ("○", "Notify: off", fg(100, 100, 100), fg(100, 100, 100)),
    }
}

fn flash_mode_label(mode: FlashMode) -> (&'static str, &'static str, String, String) {
    match mode {
        FlashMode::Persist => ("●", "Flash: persist", fg(80, 200, 120), fg(255, 255, 255)),
        FlashMode::Once => ("◐", "Flash: brief", fg(255, 200, 60), fg(255, 200, 60)),
        FlashMode::Off => ("○", "Flash: off", fg(100, 100, 100), fg(100, 100, 100)),
    }
}

/// Render a three-state toggle and register its click region.
/// Assumes the caller has already set the desired background color.
fn render_tristate(
    buf: &mut String,
    col: &mut usize,
    state_regions: &mut Vec<MenuClickRegion>,
    key: SettingKey,
    symbol: &str,
    label: &str,
    sym_color: &str,
    label_color: &str,
) {
    let region_start = *col;
    let width = display_width(symbol) + 1 + label.len();
    *col += width;

    state_regions.push(MenuClickRegion {
        start_col: region_start,
        end_col: *col,
        action: MenuAction::ToggleSetting(key),
    });

    let _ = write!(buf, "{sym_color}{symbol} {label_color}{label}");
}

fn render_settings_menu(state: &mut State, buf: &mut String, col: &mut usize) {
    // Leading space after arrow
    let _ = write!(buf, " ");
    *col += 1;

    // --- Notifications (three-state) ---
    {
        let (symbol, label, sym_color, label_color) =
            notify_mode_label(state.settings.notifications);
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::Notifications, symbol, label, &sym_color, &label_color,
        );
    }

    // --- Flash (three-state) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, label, sym_color, label_color) =
            flash_mode_label(state.settings.flash);
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::Flash, symbol, label, &sym_color, &label_color,
        );
    }

    // --- Elapsed time (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.elapsed_time;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg(80, 200, 120), fg(255, 255, 255))
        } else {
            ("○", fg(100, 100, 100), fg(100, 100, 100))
        };
        let label = if enabled { "Elapsed time: on" } else { "Elapsed time: off" };
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::ElapsedTime, symbol, label, &sym_color, &label_color,
        );
    }

    // --- Mode indicator (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.mode_indicator;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg(80, 200, 120), fg(255, 255, 255))
        } else {
            ("○", fg(100, 100, 100), fg(100, 100, 100))
        };
        let label = if enabled { "Mode indicator: on" } else { "Mode indicator: off" };
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::ModeIndicator, symbol, label, &sym_color, &label_color,
        );
    }

    // Close button
    let _ = write!(buf, "  ");
    *col += 2;
    let close_start = *col;
    let _ = write!(buf, "{}×", fg(255, 60, 60));
    *col += 1;

    state.menu_click_regions.push(MenuClickRegion {
        start_col: close_start,
        end_col: *col,
        action: MenuAction::CloseMenu,
    });
}
