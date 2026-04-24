mod event_handler;
mod installer;
mod render;
mod state;
mod tab_pane_map;

use state::{
    unix_now, unix_now_ms, Activity, AgentSource, HookPayload, MenuAction, SessionInfo, Settings,
    State, ViewMode,
};
use std::collections::BTreeMap;
use std::path::Path;
use zellij_tile::prelude::*;

const DONE_TIMEOUT: u64 = 30;
const TIMER_INTERVAL: f64 = 1.0;
const FLASH_TICK: f64 = 0.25;
const CODEX_PANE_TITLE: &str = "Codex ◎";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::ModeUpdate,
            EventType::Timer,
            EventType::Mouse,
            EventType::RunCommandResult,
            EventType::PermissionRequestResult,
        ]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
            PermissionType::ReadCliPipes,
            PermissionType::MessageAndLaunchOtherPlugins,
        ]);
        set_timeout(TIMER_INTERVAL);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                let new_active = tabs.iter().find(|t| t.active).map(|t| t.position);
                if new_active != self.active_tab_index {
                    // Tab focus changed — clear persist flashes on the newly focused tab
                    if let Some(idx) = new_active {
                        self.clear_flashes_on_tab(idx);
                    }
                }
                self.active_tab_index = new_active;
                self.tabs = tabs;
                self.rebuild_pane_map();
                true
            }
            Event::PaneUpdate(manifest) => {
                self.pane_manifest = Some(manifest);
                self.rebuild_pane_map();
                true
            }
            Event::ModeUpdate(mode_info) => {
                self.input_mode = mode_info.mode;
                if let Some(name) = mode_info.session_name {
                    self.zellij_session_name = Some(name);
                }
                true
            }
            Event::Mouse(Mouse::LeftClick(_, col)) => {
                let col = col as usize;

                // Check prefix click region first → toggle ViewMode
                if let Some((start, end)) = self.prefix_click_region {
                    if col >= start && col < end {
                        self.view_mode = match self.view_mode {
                            ViewMode::Normal => ViewMode::Settings,
                            ViewMode::Settings => ViewMode::Normal,
                        };
                        return true;
                    }
                }

                match self.view_mode {
                    ViewMode::Normal => {
                        for region in &self.click_regions {
                            if col >= region.start_col && col < region.end_col {
                                if region.is_waiting {
                                    focus_terminal_pane(region.pane_id, false, false);
                                } else {
                                    switch_tab_to(region.tab_index as u32 + 1);
                                }
                                return false;
                            }
                        }
                        false
                    }
                    ViewMode::Settings => {
                        for region in &self.menu_click_regions {
                            if col >= region.start_col && col < region.end_col {
                                match &region.action {
                                    MenuAction::ToggleSetting(key) => {
                                        match key {
                                            state::SettingKey::Notifications => {
                                                self.settings.notifications =
                                                    self.settings.notifications.cycle();
                                            }
                                            state::SettingKey::Flash => {
                                                self.settings.flash = self.settings.flash.cycle();
                                            }
                                            state::SettingKey::ElapsedTime => {
                                                self.settings.elapsed_time =
                                                    !self.settings.elapsed_time;
                                            }
                                            state::SettingKey::ModeIndicator => {
                                                self.settings.mode_indicator =
                                                    !self.settings.mode_indicator;
                                            }
                                            state::SettingKey::Cwd => {
                                                self.settings.cwd = !self.settings.cwd;
                                            }
                                        }
                                        self.save_config();
                                    }
                                    MenuAction::CloseMenu => {
                                        self.view_mode = ViewMode::Normal;
                                    }
                                }
                                return true;
                            }
                        }
                        false
                    }
                }
            }
            Event::RunCommandResult(exit_code, stdout, _stderr, context) => {
                match context.get("type").map(|s| s.as_str()) {
                    Some("load_config") if exit_code == Some(0) => {
                        let raw = String::from_utf8_lossy(&stdout);
                        if let Ok(settings) = serde_json::from_str::<Settings>(raw.trim()) {
                            self.settings = settings;
                        }
                        self.config_loaded = true;
                        true
                    }
                    Some("install_hooks") => {
                        self.hooks_installed = true;
                        false
                    }
                    Some("install_codex_hooks") => {
                        self.codex_hooks_installed = true;
                        false
                    }
                    _ => false,
                }
            }
            Event::Timer(_) => {
                let inferred_changed = self.refresh_agent_sessions_from_manifest();
                let stale_changed = self.cleanup_stale_sessions();
                let flash_changed = self.cleanup_expired_flashes();
                let has_flashes = self.has_active_flashes();
                if has_flashes {
                    set_timeout(FLASH_TICK);
                } else {
                    set_timeout(TIMER_INTERVAL);
                }
                has_flashes
                    || inferred_changed
                    || stale_changed
                    || flash_changed
                    || self.has_elapsed_display()
            }
            Event::PermissionRequestResult(status) => {
                // Keep the pane visible during fullscreen regardless of status.
                set_selectable(false);
                if status == PermissionStatus::Granted {
                    // Permissions granted — ask existing instances for their state.
                    self.request_sync();
                    if !self.config_loaded {
                        self.load_config();
                    }
                    // Auto-install hook scripts and register hooks for both CLIs.
                    if !self.hooks_installed {
                        installer::run_install();
                    }
                    if !self.codex_hooks_installed {
                        installer::run_codex_install();
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        match pipe_message.name.as_str() {
            "zellaude" => {
                // Hook event from CLI
                let payload_str = match pipe_message.payload {
                    Some(ref s) => s,
                    None => return false,
                };
                let payload: HookPayload = match serde_json::from_str(payload_str) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                event_handler::handle_hook_event(self, payload);
                true
            }
            "zellaude:focus" => {
                // Notification click — focus the requested pane
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(pane_id) = payload.trim().parse::<u32>() {
                        focus_terminal_pane(pane_id, false, false);
                    }
                }
                false
            }
            "zellaude:request" => {
                // Another instance asking for state — respond with ours
                self.broadcast_sessions();
                false
            }
            "zellaude:settings" => {
                // Another instance broadcast new settings
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(settings) = serde_json::from_str::<Settings>(payload) {
                        self.settings = settings;
                        return true;
                    }
                }
                false
            }
            "zellaude:sync" => {
                // Another instance sharing state — merge it
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(sessions) =
                        serde_json::from_str::<BTreeMap<u32, SessionInfo>>(payload)
                    {
                        self.merge_sessions(sessions);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        render::render_status_bar(self, rows, cols);
    }
}

impl State {
    fn rebuild_pane_map(&mut self) {
        if let Some(ref manifest) = self.pane_manifest {
            self.pane_to_tab = tab_pane_map::build_pane_to_tab_map(&self.tabs, manifest);
            self.refresh_session_tab_names();
            self.remove_dead_panes();
            self.refresh_agent_sessions_from_manifest();
        }
    }

    fn refresh_session_tab_names(&mut self) {
        for session in self.sessions.values_mut() {
            if let Some((idx, name)) = self.pane_to_tab.get(&session.pane_id) {
                session.tab_index = Some(*idx);
                session.tab_name = Some(name.clone());
            }
        }
    }

    fn remove_dead_panes(&mut self) {
        self.sessions
            .retain(|pane_id, _| self.pane_to_tab.contains_key(pane_id));
    }

    fn refresh_agent_sessions_from_manifest(&mut self) -> bool {
        let Some(manifest) = self.pane_manifest.clone() else {
            return false;
        };

        let now = unix_now();
        let mut changed = false;

        for panes in manifest.panes.values() {
            for pane in panes {
                if pane.is_plugin {
                    continue;
                }

                let pane_id = pane.id;
                let Some(source) = Self::infer_agent_source(
                    pane_id,
                    &pane.title,
                    pane.terminal_command.as_deref(),
                ) else {
                    continue;
                };

                let fallback_activity = Self::fallback_activity(source, &pane.title);
                let tab_info = self.pane_to_tab.get(&pane_id).cloned();
                let observed_cwd = get_pane_cwd(PaneId::Terminal(pane_id))
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());

                if source == AgentSource::Codex {
                    Self::maybe_rename_codex_pane(pane_id, &pane.title, observed_cwd.as_deref());
                }

                if let Some(session) = self.sessions.get_mut(&pane_id) {
                    if session.source != source {
                        session.source = source;
                        changed = true;
                    }

                    // Keep inferred sessions synchronized to title-derived activity, but
                    // never override hook-driven sessions.
                    if session.session_id.is_empty() {
                        if session.activity != fallback_activity {
                            session.activity = fallback_activity;
                            session.last_event_ts = now;
                            changed = true;
                        }
                        if session.last_event_ts == 0 {
                            session.last_event_ts = now;
                            changed = true;
                        }
                        if session.cwd.is_none() {
                            if let Some(cwd) = observed_cwd.clone() {
                                session.cwd = Some(cwd);
                            }
                        }
                    }

                    if let Some((idx, name)) = tab_info {
                        if session.tab_index != Some(idx) {
                            session.tab_index = Some(idx);
                            changed = true;
                        }
                        if session.tab_name.as_deref() != Some(name.as_str()) {
                            session.tab_name = Some(name);
                            changed = true;
                        }
                    }
                } else {
                    let (tab_index, tab_name) = match tab_info {
                        Some((idx, name)) => (Some(idx), Some(name)),
                        None => (None, None),
                    };
                    self.sessions.insert(
                        pane_id,
                        SessionInfo {
                            session_id: String::new(),
                            pane_id,
                            activity: fallback_activity,
                            tab_name,
                            tab_index,
                            last_event_ts: now,
                            cwd: observed_cwd.clone(),
                            // Keep ts at 0 for inferred sessions so real hook events take precedence.
                            last_ts_ms: 0,
                            source,
                        },
                    );
                    changed = true;
                }
            }
        }

        changed
    }

    fn infer_agent_source(
        pane_id: u32,
        title: &str,
        terminal_command: Option<&str>,
    ) -> Option<AgentSource> {
        if let Some(command) = terminal_command {
            if Self::command_matches(command, "codex") {
                return Some(AgentSource::Codex);
            }
            if Self::command_matches(command, "claude") {
                return Some(AgentSource::Claude);
            }
        }

        if let Ok(argv) = get_pane_running_command(PaneId::Terminal(pane_id)) {
            if Self::argv_matches(&argv, "codex") {
                return Some(AgentSource::Codex);
            }
            if Self::argv_matches(&argv, "claude") {
                return Some(AgentSource::Claude);
            }
        }

        let title_lc = title.to_ascii_lowercase();
        if title_lc.contains("claude") || title.starts_with('✳') {
            return Some(AgentSource::Claude);
        }
        if Self::title_has_spinner(title) {
            return Some(AgentSource::Codex);
        }
        None
    }

    fn command_matches(command: &str, bin: &str) -> bool {
        let lower = command.to_ascii_lowercase();
        if lower.contains(&format!("/{bin}")) || lower.contains(&format!("\\{bin}")) {
            return true;
        }

        command.split_whitespace().any(|token| {
            let token = token.trim_matches(|c| c == '"' || c == '\'');
            Path::new(token)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| Self::name_matches_bin(name, bin))
                .unwrap_or(false)
        })
    }

    fn argv_matches(argv: &[String], bin: &str) -> bool {
        argv.iter().any(|arg| {
            let lower = arg.to_ascii_lowercase();
            if lower.contains(&format!("/{bin}")) || lower.contains(&format!("\\{bin}")) {
                return true;
            }
            Path::new(arg)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| Self::name_matches_bin(name, bin))
                .unwrap_or(false)
        })
    }

    fn name_matches_bin(name: &str, bin: &str) -> bool {
        let base = name.to_ascii_lowercase();
        base == bin
            || base == format!("{bin}.exe")
            || base == format!("{bin}.js")
            || base.starts_with(&format!("{bin}-"))
            || base.starts_with(&format!("{bin}_"))
    }

    fn title_has_spinner(title: &str) -> bool {
        let Some(first) = title.chars().next() else {
            return false;
        };
        ('\u{2800}'..='\u{28ff}').contains(&first)
    }

    fn maybe_rename_codex_pane(pane_id: u32, title: &str, cwd: Option<&str>) {
        let visible_title = Self::title_without_spinner(title).trim();

        if visible_title == CODEX_PANE_TITLE {
            return;
        }

        if visible_title.eq_ignore_ascii_case("Codex") {
            rename_terminal_pane(pane_id, CODEX_PANE_TITLE);
            return;
        }

        if visible_title.to_ascii_lowercase().contains("codex") {
            return;
        }

        let Some(leaf) = Self::cwd_leaf(cwd) else {
            return;
        };

        if visible_title.eq_ignore_ascii_case(leaf) {
            rename_terminal_pane(pane_id, CODEX_PANE_TITLE);
        }
    }

    fn title_without_spinner(title: &str) -> &str {
        let title = title.trim();
        let mut chars = title.chars();
        if let Some(first) = chars.next() {
            if ('\u{2800}'..='\u{28ff}').contains(&first) {
                return chars.as_str().trim_start();
            }
        }
        title
    }

    fn cwd_leaf(cwd: Option<&str>) -> Option<&str> {
        let cwd = cwd?.trim();
        if cwd.is_empty() {
            return None;
        }
        if cwd == "/" {
            return Some("/");
        }
        let trimmed = cwd.trim_end_matches('/');
        if trimmed.is_empty() {
            Some("/")
        } else {
            Some(trimmed.rsplit('/').next().unwrap_or(trimmed))
        }
    }

    fn fallback_activity(source: AgentSource, title: &str) -> Activity {
        match source {
            AgentSource::Codex => {
                if Self::title_has_spinner(title) {
                    Activity::Thinking
                } else {
                    Activity::Idle
                }
            }
            AgentSource::Claude => {
                if title.starts_with('✳') {
                    Activity::Thinking
                } else {
                    Activity::Idle
                }
            }
        }
    }

    fn cleanup_stale_sessions(&mut self) -> bool {
        let now = unix_now();
        let mut changed = false;
        for session in self.sessions.values_mut() {
            match session.activity {
                state::Activity::Done | state::Activity::AgentDone => {
                    if now.saturating_sub(session.last_event_ts) >= DONE_TIMEOUT {
                        session.activity = state::Activity::Idle;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }

    fn clear_flashes_on_tab(&mut self, tab_idx: usize) {
        let pane_ids: Vec<u32> = self
            .sessions
            .values()
            .filter(|s| s.tab_index == Some(tab_idx))
            .map(|s| s.pane_id)
            .collect();
        for pane_id in pane_ids {
            self.flash_deadlines.remove(&pane_id);
        }
    }

    fn has_active_flashes(&self) -> bool {
        let now = unix_now_ms();
        self.flash_deadlines
            .values()
            .any(|&deadline| now < deadline)
    }

    fn cleanup_expired_flashes(&mut self) -> bool {
        let before = self.flash_deadlines.len();
        let now = unix_now_ms();
        self.flash_deadlines.retain(|_, deadline| now < *deadline);
        self.flash_deadlines.len() != before
    }

    fn has_elapsed_display(&self) -> bool {
        if !self.settings.elapsed_time {
            return false;
        }
        let now = unix_now();
        self.sessions.values().any(|s| {
            !matches!(s.activity, state::Activity::Idle)
                && now.saturating_sub(s.last_event_ts) >= DONE_TIMEOUT
        })
    }

    fn request_sync(&self) {
        pipe_message_to_plugin(MessageToPlugin::new("zellaude:request"));
    }

    fn broadcast_sessions(&self) {
        let mut msg = MessageToPlugin::new("zellaude:sync");
        msg.message_payload = Some(serde_json::to_string(&self.sessions).unwrap_or_default());
        pipe_message_to_plugin(msg);
    }

    fn broadcast_settings(&self) {
        let mut msg = MessageToPlugin::new("zellaude:settings");
        msg.message_payload = Some(serde_json::to_string(&self.settings).unwrap_or_default());
        pipe_message_to_plugin(msg);
    }

    fn load_config(&self) {
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "load_config".into());
        run_command(
            &[
                "sh",
                "-c",
                "cat \"$HOME/.config/zellij/plugins/zellaude.json\" 2>/dev/null || echo '{}'",
            ],
            ctx,
        );
    }

    fn save_config(&self) {
        if !self.config_loaded {
            return;
        }
        self.broadcast_settings();
        let json = serde_json::to_string(&self.settings).unwrap_or_default();
        let json_esc = json.replace('\'', "'\\''");
        let cmd = format!(
            "mkdir -p \"$HOME/.config/zellij/plugins\" && printf '%s' '{json_esc}' > \"$HOME/.config/zellij/plugins/zellaude.json\""
        );
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "save_config".into());
        run_command(&["sh", "-c", &cmd], ctx);
    }

    fn merge_sessions(&mut self, incoming: BTreeMap<u32, SessionInfo>) {
        for (pane_id, mut session) in incoming {
            let dominated = self
                .sessions
                .get(&pane_id)
                .map(|existing| session.last_event_ts > existing.last_event_ts)
                .unwrap_or(true);
            if dominated {
                // Refresh tab name from our local pane map
                if let Some((idx, name)) = self.pane_to_tab.get(&pane_id) {
                    session.tab_index = Some(*idx);
                    session.tab_name = Some(name.clone());
                }
                self.sessions.insert(pane_id, session);
            }
        }
    }
}
