use crate::state::{
    unix_now, unix_now_ms, Activity, AgentSource, BuddyStyle, ClickRegion, FlashMode, MenuAction,
    MenuClickRegion, NotifyMode, SessionInfo, SettingKey, State, ViewMode,
};
use std::fmt::Write;
use std::io::Write as IoWrite;
use zellij_tile::prelude::{InputMode, TabInfo};

type Color = (u8, u8, u8);

const THEME_FG: Color = (212, 212, 212);
const THEME_BG: Color = (30, 30, 30);
const THEME_BLACK: Color = (37, 37, 38);
const THEME_RED: Color = (160, 37, 58);
const THEME_GREEN: Color = (155, 58, 72);
const THEME_YELLOW: Color = (220, 220, 170);
const THEME_BLUE: Color = (194, 74, 90);
const THEME_MAGENTA: Color = (197, 134, 192);
const THEME_CYAN: Color = (78, 201, 176);
const THEME_WHITE: Color = (212, 212, 212);
const THEME_ORANGE: Color = (206, 145, 120);

struct Style {
    symbol: &'static str,
    r: u8,
    g: u8,
    b: u8,
}

const CODEX_MARK: &str = "◎";
const BUDDY_WIDTH: usize = 9; // 1 space + 8-char expression (5-char face + 3-char accessory zone)
const BUDDY_SPEECH_WIDTH: usize = 20; // speech text zone to the left of the buddy

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
        "Bash" | "shell" => "⚡",
        "Read" | "Glob" | "Grep" => "◉",
        "Edit" | "Write" => "✎",
        "Task" => "⊜",
        "WebSearch" | "WebFetch" => "◈",
        _ => "⚙",
    }
}

fn style(symbol: &'static str, color: Color) -> Style {
    Style {
        symbol,
        r: color.0,
        g: color.1,
        b: color.2,
    }
}

fn dim(color: Color, pct: u32) -> Color {
    (
        (color.0 as u32 * pct / 100) as u8,
        (color.1 as u32 * pct / 100) as u8,
        (color.2 as u32 * pct / 100) as u8,
    )
}

// Claude uses the theme's orange/yellow/red accents.
fn activity_style_claude(activity: &Activity) -> Style {
    match activity {
        Activity::Init => style("◆", dim(THEME_WHITE, 82)),
        Activity::Thinking => style("●", THEME_ORANGE),
        Activity::Tool(name) => style(tool_symbol(name), THEME_ORANGE),
        Activity::Prompting => style("▶", THEME_CYAN),
        Activity::Waiting => style("⚠", THEME_YELLOW),
        Activity::Notification => style("◇", THEME_YELLOW),
        Activity::Done => style("✓", THEME_CYAN),
        Activity::AgentDone => style("✓", THEME_GREEN),
        Activity::Idle => style("○", dim(THEME_FG, 72)),
    }
}

// Codex uses the theme's cyan accent.
fn activity_style_codex(activity: &Activity) -> Style {
    match activity {
        Activity::Init => style(CODEX_MARK, dim(THEME_CYAN, 88)),
        Activity::Thinking => style("●", THEME_CYAN),
        Activity::Tool(name) => style(tool_symbol(name), THEME_CYAN),
        Activity::Prompting => style("▶", THEME_CYAN),
        Activity::Waiting => style("⚠", THEME_YELLOW),
        Activity::Notification => style("◇", THEME_CYAN),
        Activity::Done => style(CODEX_MARK, THEME_CYAN),
        Activity::AgentDone => style(CODEX_MARK, dim(THEME_CYAN, 82)),
        Activity::Idle => style(CODEX_MARK, dim(THEME_CYAN, 78)),
    }
}

fn activity_style(activity: &Activity, source: AgentSource) -> Style {
    match source {
        AgentSource::Claude => activity_style_claude(activity),
        AgentSource::Codex => activity_style_codex(activity),
    }
}

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn fg_color(color: Color) -> String {
    fg(color.0, color.1, color.2)
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ELAPSED_THRESHOLD: u64 = 30;
const PILL_LEFT_CAP: &str = "\u{e0b6}";
const PILL_RIGHT_CAP: &str = "\u{e0b4}";

const BAR_BG: Color = THEME_BG;
const PREFIX_BG: Color = THEME_RED;
const PREFIX_BG_SETTINGS: Color = THEME_BLUE;
const TAB_BG_ACTIVE: Color = THEME_BLUE;
const TAB_BG_INACTIVE: Color = THEME_BLACK;
const FLASH_BG_BRIGHT: Color = THEME_YELLOW;

fn high_contrast_text_color(bg: Color) -> Color {
    let (r, g, b) = bg;
    // Perceived brightness (integer approximation): use dark text on bright fills.
    let luminance = (u16::from(r) * 299 + u16::from(g) * 587 + u16::from(b) * 114) / 1000;
    if luminance >= 165 {
        (20, 20, 20)
    } else {
        (255, 255, 255)
    }
}

fn pill_width(text: &str) -> usize {
    display_width(text) + 2
}

fn pill_open(buf: &mut String, col: &mut usize, pill_bg: Color) {
    let _ = write!(
        buf,
        "{}{}{PILL_LEFT_CAP}",
        fg(pill_bg.0, pill_bg.1, pill_bg.2),
        bg(BAR_BG.0, BAR_BG.1, BAR_BG.2),
    );
    *col += 1;
}

fn pill_close(buf: &mut String, col: &mut usize, pill_bg: Color) {
    let _ = write!(
        buf,
        "{}{}{PILL_RIGHT_CAP}",
        fg(pill_bg.0, pill_bg.1, pill_bg.2),
        bg(BAR_BG.0, BAR_BG.1, BAR_BG.2),
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

fn format_cwd_label(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }

    let leaf = if cwd == "/" {
        "/"
    } else {
        let trimmed = cwd.trim_end_matches('/');
        if trimmed.is_empty() {
            "/"
        } else {
            trimmed.rsplit('/').next().unwrap_or(trimmed)
        }
    };

    let max_len = 14;
    let char_count = leaf.chars().count();
    if char_count > max_len {
        let short: String = leaf.chars().take(max_len.saturating_sub(1)).collect();
        Some(format!("{short}…"))
    } else {
        Some(leaf.to_string())
    }
}

fn mode_style(mode: InputMode) -> (Color, &'static str) {
    match mode {
        InputMode::Normal => (THEME_BLUE, "NORMAL"),
        InputMode::Locked => (THEME_RED, "LOCKED"),
        InputMode::Pane => (THEME_CYAN, "PANE"),
        InputMode::Tab => (THEME_MAGENTA, "TAB"),
        InputMode::Resize => (THEME_ORANGE, "RESIZE"),
        InputMode::Move => (THEME_ORANGE, "MOVE"),
        InputMode::Scroll => (THEME_YELLOW, "SCROLL"),
        InputMode::EnterSearch => (THEME_YELLOW, "SEARCH"),
        InputMode::Search => (THEME_YELLOW, "SEARCH"),
        InputMode::RenameTab => (THEME_YELLOW, "RENAME"),
        InputMode::RenamePane => (THEME_YELLOW, "RENAME"),
        InputMode::Session => (THEME_MAGENTA, "SESSION"),
        InputMode::Prompt => (THEME_CYAN, "PROMPT"),
        InputMode::Tmux => (THEME_CYAN, "TMUX"),
    }
}

fn dominant_activity(state: &State) -> (Activity, AgentSource) {
    state
        .sessions
        .values()
        .max_by(|a, b| {
            activity_priority(&a.activity)
                .cmp(&activity_priority(&b.activity))
                .then(a.last_event_ts.cmp(&b.last_event_ts))
        })
        .map(|s| (s.activity.clone(), s.source))
        .unwrap_or((Activity::Idle, AgentSource::Claude))
}

// Each entry is exactly 8 printable ASCII chars: 5-char face + 3-char accessory zone.
fn buddy_faces(activity: &Activity, frame: u8) -> (&'static str, (u8, u8, u8)) {
    let f = frame as usize;
    match activity {
        // Idle: 8-frame cycle at 500ms — 7 open + 1 blink (frame 7)
        Activity::Idle => {
            if f % 8 == 7 {
                ("(-_-)   ", dim(THEME_FG, 62))
            } else {
                ("(^-^)   ", dim(THEME_FG, 62))
            }
        }
        Activity::Init => ("(?_?)   ", dim(THEME_FG, 82)),
        Activity::Prompting => ("(~_~)   ", THEME_CYAN),
        Activity::Notification => ("(oAo)   ", THEME_YELLOW),
        Activity::Done => ("(^v^)   ", THEME_CYAN),
        Activity::AgentDone => ("(-v-)   ", THEME_GREEN),
        Activity::Tool(_) => ("(>v<)   ", THEME_ORANGE),
        // Thinking: 4-frame cycle at 250ms — dots grow left to right
        Activity::Thinking => {
            let frames = ["(*_*)   ", "(o_o).  ", "(*_*).. ", "(o_o)..."];
            (frames[f % 4], THEME_ORANGE)
        }
        // Waiting: 4-frame cycle at 250ms — exclamations escalate
        Activity::Waiting => {
            let frames = ["(>_<)   ", "(>_<)!  ", "(TwT)!! ", "(TwT)!!!"];
            (frames[f % 4], THEME_YELLOW)
        }
    }
}

// Cat style: =^X^= faces (= are cheeks, ^ are ears)
fn cat_faces(activity: &Activity, frame: u8) -> (&'static str, (u8, u8, u8)) {
    let f = frame as usize;
    match activity {
        // Idle: 8-frame cycle at 500ms — 7 open + 1 blink (frame 7)
        Activity::Idle => {
            if f % 8 == 7 {
                ("=^-^=   ", dim(THEME_FG, 62))
            } else {
                ("=^.^=   ", dim(THEME_FG, 62))
            }
        }
        Activity::Init => ("=^o^=   ", dim(THEME_FG, 82)),
        Activity::Prompting => ("=^,^=   ", THEME_CYAN),
        Activity::Notification => ("=^!^=   ", THEME_YELLOW),
        Activity::Done => ("=^v^=   ", THEME_CYAN),
        Activity::AgentDone => ("=^u^=   ", THEME_GREEN),
        Activity::Tool(_) => ("=^>^=   ", THEME_ORANGE),
        // Thinking: dots grow across the accessory zone
        Activity::Thinking => {
            let frames = ["=^*^=   ", "=^*^=.  ", "=^o^=.. ", "=^*^=..."];
            (frames[f % 4], THEME_ORANGE)
        }
        // Waiting: distressed cat, exclamations escalate
        Activity::Waiting => {
            let frames = ["=ToT=   ", "=ToT=!  ", "=ToT=!! ", "=ToT=!!!"];
            (frames[f % 4], THEME_YELLOW)
        }
    }
}

fn kaomoji_speech(activity: &Activity, frame: u8, speech_var: usize) -> &'static str {
    match activity {
        Activity::Idle => {
            if frame % 8 == 7 {
                "*blink blink*"
            } else {
                let lines = [
                    "just vibing...",
                    "watching u code",
                    "",
                    "no bugs yet. bold.",
                    "...still here",
                    "",
                    "coffee depleted",
                    "*stares at code*",
                    "",
                    "u got this",
                    "try not to break it",
                    "",
                ];
                lines[speech_var % lines.len()]
            }
        }
        Activity::Thinking => {
            let lines = [
                "hang on...",
                "",
                "crunching...",
                "",
                "this is fine...",
                "",
                "almost!",
                "",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Waiting => {
            let lines = [
                "your move...",
                "",
                "hello?",
                "",
                "anytime now...",
                "HELLO?!",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Done => {
            let lines = [
                "nailed it!",
                "ez. next?",
                "",
                "was never in doubt",
                "ship it",
                "",
                "flawless",
                "clean.",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Tool(_) => {
            let lines = [
                "on it!",
                "",
                "deploying minions",
                "executing...",
                "",
                "running... trust",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Init => {
            let lines = [
                "loading...",
                "booting brain...",
                "",
                "systems online",
                "waking up...",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Prompting => {
            let lines = [
                "go ahead...",
                "",
                "listening...",
                "tell me everything",
                "",
                "i'm all ears",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Notification => {
            let lines = ["heads up!", "oh btw...", "", "fyi.", "pay attention!"];
            lines[speech_var % lines.len()]
        }
        Activity::AgentDone => {
            let lines = [
                "sub returned",
                "",
                "delegation done",
                "good little agent",
                "",
                "welcome back",
            ];
            lines[speech_var % lines.len()]
        }
    }
}

fn cat_speech(activity: &Activity, frame: u8, speech_var: usize) -> &'static str {
    match activity {
        Activity::Idle => {
            if frame % 8 == 7 {
                "*yaaawn*"
            } else {
                let lines = [
                    "purrrrr...",
                    "*slow blinks*",
                    "",
                    "mrow~",
                    "*nap time*",
                    "",
                    "zzz... purr",
                    "*stretches*",
                    "",
                    "*blinks at u*",
                    "*kneads blanket*",
                    "",
                ];
                lines[speech_var % lines.len()]
            }
        }
        Activity::Thinking => {
            let lines = [
                "mrrrow...",
                "",
                "*paw on chin*",
                "",
                "mew mew mew...",
                "",
                "*pounces idea*",
                "",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Waiting => {
            let lines = ["mrow?", "", "MROW!", "", "MEOW!!", "MRRROWWW!!!"];
            lines[speech_var % lines.len()]
        }
        Activity::Done => {
            let lines = [
                "purr purr :3",
                "*happy trill*",
                "",
                "mrow! nice!",
                "*head boop*",
                "",
                "prrrfect!",
                "mrrrow~",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Tool(_) => {
            let lines = [
                "*chases cursor*",
                "",
                "pounce!!",
                "*swats bugs*",
                "",
                "mrrrow! on it!",
            ];
            lines[speech_var % lines.len()]
        }
        Activity::Init => {
            let lines = ["*yawns loudly*", "mew...", "", "*stretches paws*", "mrrp?"];
            lines[speech_var % lines.len()]
        }
        Activity::Prompting => {
            let lines = ["mrow?", "", "*tilts head*", "*perks ears*", "", "meow~"];
            lines[speech_var % lines.len()]
        }
        Activity::Notification => {
            let lines = ["mrrp!", "*ear twitch*", "", "meow!", "pspsps!"];
            lines[speech_var % lines.len()]
        }
        Activity::AgentDone => {
            let lines = [
                "*kneads paw*",
                "",
                "purr purr...",
                "good kitty help",
                "",
                "mrrrow :3",
            ];
            lines[speech_var % lines.len()]
        }
    }
}

fn render_buddy_speech(state: &State, buf: &mut String, col: &mut usize) {
    let now_ms = unix_now_ms();
    let (activity, _source) = dominant_activity(state);
    let frame = match &activity {
        Activity::Thinking | Activity::Waiting => ((now_ms / 250) % 4) as u8,
        Activity::Idle => ((now_ms / 500) % 8) as u8,
        _ => 0,
    };
    let speech_var = ((now_ms / 4000) % 16) as usize;

    let (_, (r, g, b)) = match state.settings.buddy_style {
        BuddyStyle::Cat => cat_faces(&activity, frame),
        _ => buddy_faces(&activity, frame),
    };
    let (sr, sg, sb) = (
        (r as u32 * 55 / 100) as u8,
        (g as u32 * 55 / 100) as u8,
        (b as u32 * 55 / 100) as u8,
    );

    let text = match state.settings.buddy_style {
        BuddyStyle::Cat => cat_speech(&activity, frame, speech_var),
        _ => kaomoji_speech(&activity, frame, speech_var),
    };

    let text_len = text.len();
    let padding = BUDDY_SPEECH_WIDTH.saturating_sub(text_len);
    let _ = write!(
        buf,
        "{}{}{:padding$}{}",
        bg(BAR_BG.0, BAR_BG.1, BAR_BG.2),
        fg(sr, sg, sb),
        "",
        text,
    );
    *col += BUDDY_SPEECH_WIDTH;
}

fn render_buddy(state: &State, buf: &mut String, col: &mut usize) {
    let now_ms = unix_now_ms();
    let (activity, _source) = dominant_activity(state);
    let frame = match &activity {
        Activity::Thinking | Activity::Waiting => ((now_ms / 250) % 4) as u8,
        Activity::Idle => ((now_ms / 500) % 8) as u8,
        _ => 0,
    };
    let (expr, (r, g, b)) = match state.settings.buddy_style {
        BuddyStyle::Cat => cat_faces(&activity, frame),
        _ => buddy_faces(&activity, frame),
    };
    let _ = write!(
        buf,
        "{} {}{}",
        bg(BAR_BG.0, BAR_BG.1, BAR_BG.2),
        fg(r, g, b),
        expr
    );
    *col += BUDDY_WIDTH;
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

    // Build prefix: " Zellaude (session) "
    let (mode_bg, mode_text) = mode_style(state.input_mode);
    let show_mode = state.settings.mode_indicator;
    let session_part = match state.zellij_session_name.as_deref() {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    let prefix_text = format!(" Zellaude{session_part} ");
    let mode_pill_text = format!(" {mode_text} ");
    let prefix_text_width = display_width(&prefix_text);
    let prefix_pill_width = pill_width(&prefix_text);
    let mode_pill_width = if show_mode {
        pill_width(&mode_pill_text)
    } else {
        0
    };
    let between_prefix_and_mode = if show_mode { 1 } else { 0 };
    let total_prefix_width = prefix_pill_width + between_prefix_and_mode + mode_pill_width;

    // Render prefix as rounded pills (truncate if wider than cols)
    let mut col = 0usize;
    if total_prefix_width <= cols {
        let region_start = col;
        pill_open(&mut buf, &mut col, prefix_bg);
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg_color(THEME_WHITE)
        );
        col += prefix_text_width;
        pill_close(&mut buf, &mut col, prefix_bg);

        if show_mode {
            let _ = write!(buf, "{bar_bg_str} ");
            col += 1;
            pill_open(&mut buf, &mut col, mode_bg);
            let mode_fg = high_contrast_text_color(mode_bg);
            let _ = write!(
                buf,
                "{}{}{BOLD}{mode_pill_text}",
                bg(mode_bg.0, mode_bg.1, mode_bg.2),
                fg(mode_fg.0, mode_fg.1, mode_fg.2),
            );
            col += display_width(&mode_pill_text);
            pill_close(&mut buf, &mut col, mode_bg);
        }
        state.prefix_click_region = Some((region_start, col));
    } else if prefix_pill_width <= cols {
        // Fit the name part but skip mode pill
        let region_start = col;
        pill_open(&mut buf, &mut col, prefix_bg);
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg_color(THEME_WHITE)
        );
        col += prefix_text_width;
        pill_close(&mut buf, &mut col, prefix_bg);
        state.prefix_click_region = Some((region_start, col));
    } else {
        // Even the full prefix does not fit — render a truncated pill.
        let region_start = col;
        let avail = cols.saturating_sub(2);
        let short: String = prefix_text.chars().take(avail).collect();
        pill_open(&mut buf, &mut col, prefix_bg);
        let _ = write!(
            buf,
            "{}{}{BOLD}{short}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg_color(THEME_WHITE)
        );
        col += display_width(&short);
        pill_close(&mut buf, &mut col, prefix_bg);
        state.prefix_click_region = Some((region_start, col));
    }
    let prefix_used = col;

    // Reserve buddy + speech columns on the right (Normal mode only)
    let buddy_with_speech = state.settings.buddy_style != BuddyStyle::Off
        && state.view_mode == ViewMode::Normal
        && cols > BUDDY_WIDTH + BUDDY_SPEECH_WIDTH + 5;

    let tab_cols = if buddy_with_speech {
        cols - BUDDY_WIDTH - BUDDY_SPEECH_WIDTH
    } else if state.settings.buddy_style != BuddyStyle::Off
        && state.view_mode == ViewMode::Normal
        && cols > BUDDY_WIDTH + 5
    {
        cols - BUDDY_WIDTH
    } else {
        cols
    };

    if col < cols {
        match state.view_mode {
            ViewMode::Normal => {
                render_tabs(state, &mut buf, &mut col, tab_cols, prefix_used);
            }
            ViewMode::Settings => {
                if col + 1 < cols {
                    let _ = write!(buf, "{bar_bg_str} ");
                    col += 1;
                }
                let _ = write!(buf, "{bar_bg_str}");
                render_settings_menu(state, &mut buf, &mut col);
            }
        }
    }

    // Fill from tabs end to buddy zone edge
    if col < tab_cols {
        let remaining = tab_cols - col;
        let _ = write!(buf, "{bar_bg_str}{:width$}", "", width = remaining);
        col = tab_cols;
    }

    // Render speech + buddy in reserved zone (Normal mode only, if enabled)
    if buddy_with_speech {
        render_buddy_speech(state, &mut buf, &mut col);
        render_buddy(state, &mut buf, &mut col);
    } else if state.settings.buddy_style != BuddyStyle::Off
        && state.view_mode == ViewMode::Normal
        && col + BUDDY_WIDTH <= cols
    {
        render_buddy(state, &mut buf, &mut col);
    }

    // Final fill (handles disabled buddy, settings mode, or narrow terminals)
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
    prefix_width: usize,
) {
    let now_s = unix_now();
    let now_ms = unix_now_ms();

    let mut tabs: Vec<&TabInfo> = state.tabs.iter().collect();
    tabs.sort_by_key(|t| t.position);

    let count = tabs.len();
    if count == 0 || *col + 5 > cols {
        return;
    }

    // For each tab, find the best session per source independently
    let tab_sessions: Vec<(Option<&SessionInfo>, Option<&SessionInfo>)> = tabs
        .iter()
        .map(|tab| {
            let best_claude = state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position) && s.source == AgentSource::Claude)
                .max_by_key(|s| activity_priority(&s.activity));
            let best_codex = state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position) && s.source == AgentSource::Codex)
                .max_by_key(|s| activity_priority(&s.activity));
            (best_claude, best_codex)
        })
        .collect();

    // Winning session per tab across both sources
    let winning_sessions: Vec<Option<&SessionInfo>> = tab_sessions
        .iter()
        .map(|(claude, codex)| match (claude, codex) {
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
        })
        .collect();

    // Elapsed: use the winning session
    let elapsed_strs: Vec<Option<String>> = winning_sessions
        .iter()
        .map(|best| {
            if !state.settings.elapsed_time {
                return None;
            }
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

    let cwd_strs: Vec<Option<String>> = winning_sessions
        .iter()
        .map(|best| {
            if !state.settings.cwd {
                return None;
            }
            best.and_then(|s| format_cwd_label(s.cwd.as_deref()))
        })
        .collect();

    // Overhead per tab:
    // - 2 caps + 2 inner spaces
    // - tracked symbol area (1 or 3 chars depending on sources)
    let total_elapsed_width: usize = elapsed_strs
        .iter()
        .map(|e| e.as_ref().map_or(0, |s| s.len() + 1))
        .sum();
    let total_cwd_width: usize = cwd_strs
        .iter()
        .map(|c| c.as_ref().map_or(0, |s| display_width(s) + 1))
        .sum();
    let per_tab_overhead: usize = tab_sessions
        .iter()
        .map(|(c, x)| {
            let n = c.is_some() as usize + x.is_some() as usize;
            let symbols = n + usize::from(n == 2);
            4 + symbols
        })
        .sum();
    // One bar-space before tabs and between each tab pill.
    let overhead = prefix_width + count + per_tab_overhead + total_elapsed_width + total_cwd_width;
    let max_name_len = if overhead < cols {
        ((cols - overhead) / count).min(20)
    } else {
        0
    };

    let bar_bg_str = bg(BAR_BG.0, BAR_BG.1, BAR_BG.2);
    let _ = write!(buf, "{bar_bg_str} ");
    *col += 1;

    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            if *col + 5 > cols {
                break;
            }
            let _ = write!(buf, "{bar_bg_str} ");
            *col += 1;
        }

        if *col + 4 > cols {
            break;
        }

        let (claude_session, codex_session) = tab_sessions[i];
        let is_tracked = claude_session.is_some() || codex_session.is_some();
        let tab_name = &tab.name;

        let char_count = tab_name.chars().count();
        let truncated = if max_name_len == 0 {
            String::new()
        } else if char_count > max_name_len {
            let s: String = tab_name
                .chars()
                .take(max_name_len.saturating_sub(1))
                .collect();
            format!("{s}…")
        } else {
            tab_name.to_string()
        };

        let is_flash_bright = state
            .sessions
            .values()
            .filter(|s| s.tab_index == Some(tab.position))
            .any(|s| {
                state
                    .flash_deadlines
                    .get(&s.pane_id)
                    .map(|&deadline| now_ms < deadline && (now_ms / 250) % 2 == 0)
                    .unwrap_or(false)
            });

        let is_active = tab.active;
        let tab_bg = if is_flash_bright {
            FLASH_BG_BRIGHT
        } else if is_active {
            TAB_BG_ACTIVE
        } else {
            TAB_BG_INACTIVE
        };

        let region_start = *col;
        pill_open(buf, col, tab_bg);
        let tab_bg_str = bg(tab_bg.0, tab_bg.1, tab_bg.2);
        let _ = write!(buf, "{tab_bg_str}");

        if is_tracked {
            // Winning session determines name styling
            let winning = match winning_sessions[i] {
                Some(session) => session,
                None => unreachable!(),
            };

            let (name_fg, name_bold) = if is_flash_bright {
                (fg_color(THEME_RED), true)
            } else if is_active {
                (fg_color(THEME_WHITE), true)
            } else {
                let inactive = match winning.source {
                    AgentSource::Claude => fg_color(THEME_ORANGE),
                    AgentSource::Codex => fg_color(THEME_CYAN),
                };
                (inactive, false)
            };

            // Leading space
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            // Claude symbol
            if let Some(s) = claude_session {
                let style = activity_style(&s.activity, AgentSource::Claude);
                let sym_fg = if is_flash_bright {
                    fg_color(THEME_RED)
                } else {
                    fg(style.r, style.g, style.b)
                };
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
                let sym_fg = if is_flash_bright {
                    fg_color(THEME_RED)
                } else {
                    fg(style.r, style.g, style.b)
                };
                let _ = write!(buf, "{sym_fg}{}", style.symbol);
                *col += display_width(style.symbol);
            }

            // Space + name
            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, " {bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += 1 + display_width(&truncated);
            }

            if let Some(ref cwd) = cwd_strs[i] {
                let cwd_width = display_width(cwd);
                if *col + 1 + cwd_width + 2 < cols {
                    let meta_fg = high_contrast_text_color(tab_bg);
                    let _ = write!(buf, " {}{cwd}", fg(meta_fg.0, meta_fg.1, meta_fg.2));
                    *col += 1 + cwd_width;
                }
            }

            // Elapsed
            if let Some(ref es) = elapsed_strs[i] {
                if *col + 1 + es.len() + 2 < cols {
                    let meta_fg = high_contrast_text_color(tab_bg);
                    let _ = write!(buf, " {}{es}", fg(meta_fg.0, meta_fg.1, meta_fg.2));
                    *col += 1 + es.len();
                }
            }

            // Fullscreen indicator
            if tab.is_fullscreen_active && *col + 4 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg_color(THEME_YELLOW));
                *col += 2;
            }

            // Trailing space
            let _ = write!(buf, " ");
            *col += 1;

            let waiting_session = state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position))
                .find(|s| matches!(s.activity, Activity::Waiting));

            pill_close(buf, col, tab_bg);

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: waiting_session.map_or(0, |s| s.pane_id),
                is_waiting: waiting_session.is_some(),
            });
        } else {
            // Untracked tab — no symbol, dimmer name
            let name_fg = if is_active {
                fg_color(THEME_WHITE)
            } else {
                fg_color(dim(THEME_FG, 72))
            };
            let name_bold = is_active;

            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, "{bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += display_width(&truncated);
            }

            if tab.is_fullscreen_active && *col + 4 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg_color(THEME_YELLOW));
                *col += 2;
            }

            let _ = write!(buf, " ");
            *col += 1;

            pill_close(buf, col, tab_bg);

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: 0,
                is_waiting: false,
            });
        }
    }
}

fn notify_mode_label(mode: NotifyMode) -> (&'static str, &'static str, String, String) {
    match mode {
        NotifyMode::Always => (
            "●",
            "Notify: always",
            fg_color(THEME_CYAN),
            fg_color(THEME_FG),
        ),
        NotifyMode::Unfocused => (
            "◐",
            "Notify: unfocused",
            fg_color(THEME_ORANGE),
            fg_color(THEME_ORANGE),
        ),
        NotifyMode::Never => (
            "○",
            "Notify: off",
            fg_color(dim(THEME_FG, 52)),
            fg_color(dim(THEME_FG, 52)),
        ),
    }
}

fn flash_mode_label(mode: FlashMode) -> (&'static str, &'static str, String, String) {
    match mode {
        FlashMode::Persist => (
            "●",
            "Flash: persist",
            fg_color(THEME_CYAN),
            fg_color(THEME_FG),
        ),
        FlashMode::Once => (
            "◐",
            "Flash: brief",
            fg_color(THEME_ORANGE),
            fg_color(THEME_ORANGE),
        ),
        FlashMode::Off => (
            "○",
            "Flash: off",
            fg_color(dim(THEME_FG, 52)),
            fg_color(dim(THEME_FG, 52)),
        ),
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
    // Leading space after prefix
    let _ = write!(buf, " ");
    *col += 1;

    // --- Notifications (three-state) ---
    {
        let (symbol, label, sym_color, label_color) =
            notify_mode_label(state.settings.notifications);
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Notifications,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Flash (three-state) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, label, sym_color, label_color) = flash_mode_label(state.settings.flash);
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Flash,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Elapsed time (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.elapsed_time;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg_color(THEME_CYAN), fg_color(THEME_FG))
        } else {
            (
                "○",
                fg_color(dim(THEME_FG, 52)),
                fg_color(dim(THEME_FG, 52)),
            )
        };
        let label = if enabled {
            "Elapsed time: on"
        } else {
            "Elapsed time: off"
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::ElapsedTime,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Mode indicator (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.mode_indicator;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg_color(THEME_CYAN), fg_color(THEME_FG))
        } else {
            (
                "○",
                fg_color(dim(THEME_FG, 52)),
                fg_color(dim(THEME_FG, 52)),
            )
        };
        let label = if enabled {
            "Mode indicator: on"
        } else {
            "Mode indicator: off"
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::ModeIndicator,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- CWD (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.cwd;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg_color(THEME_CYAN), fg_color(THEME_FG))
        } else {
            (
                "○",
                fg_color(dim(THEME_FG, 52)),
                fg_color(dim(THEME_FG, 52)),
            )
        };
        let label = if enabled { "CWD: on" } else { "CWD: off" };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Cwd,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Buddy (three-state: off / kaomoji / cat) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, label, sym_color, label_color) = match state.settings.buddy_style {
            BuddyStyle::Off => (
                "○",
                "Buddy: off",
                fg_color(dim(THEME_FG, 52)),
                fg_color(dim(THEME_FG, 52)),
            ),
            BuddyStyle::Kaomoji => (
                "●",
                "Buddy: kaomoji",
                fg_color(THEME_CYAN),
                fg_color(THEME_FG),
            ),
            BuddyStyle::Cat => (
                "◐",
                "Buddy: cat",
                fg_color(THEME_ORANGE),
                fg_color(THEME_FG),
            ),
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Buddy,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // Close button
    let _ = write!(buf, "  ");
    *col += 2;
    let close_start = *col;
    let _ = write!(buf, "{}×", fg_color(THEME_RED));
    *col += 1;

    state.menu_click_regions.push(MenuClickRegion {
        start_col: close_start,
        end_col: *col,
        action: MenuAction::CloseMenu,
    });
}
