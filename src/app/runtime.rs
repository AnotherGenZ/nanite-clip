use super::*;

impl App {
    pub(super) fn handle_runtime_message(&mut self, message: RuntimeMessage) -> Task<Message> {
        match message {
            RuntimeMessage::StartMonitoring => {
                self.runtime.lifecycle = AppState::WaitingForGame;
                self.rule_engine.reset();
                self.stop_recorder_if_running();
                self.runtime.tracked_alerts.clear();
                self.runtime.manual_profile_override_profile_id = None;
                self.runtime.last_auto_switch_rule_id = None;
                self.runtime.startup_probe_due_at = None;
                self.runtime.startup_probe_pending_result = false;
                self.runtime.startup_probe_resolution = None;
                self.runtime.obs_connection_status = None;
                self.sync_tray_snapshot()
            }
            RuntimeMessage::StopMonitoring => {
                self.runtime.lifecycle = AppState::Idle;
                self.rule_engine.reset();
                self.runtime.honu_session_id = None;
                self.stop_recorder_if_running();
                self.runtime.tracked_alerts.clear();
                self.runtime.manual_profile_override_profile_id = None;
                self.runtime.last_auto_switch_rule_id = None;
                self.runtime.startup_probe_due_at = None;
                self.runtime.startup_probe_pending_result = false;
                self.runtime.startup_probe_resolution = None;
                self.runtime.obs_connection_status = None;
                Task::batch([self.finish_active_session(), self.sync_tray_snapshot()])
            }
            RuntimeMessage::RuntimePoll => {
                self.dismiss_expired_feedback();
                self.toasts.tick();
                let mut tasks = Vec::new();

                if matches!(self.runtime.lifecycle, AppState::Monitoring { .. }) {
                    for action in self.rule_engine.poll_due(Utc::now()) {
                        tasks.push(Task::done(Message::RuleClipAction(action)));
                    }
                }

                for result in self.recorder.poll_save_results() {
                    if self.runtime.startup_probe_pending_result {
                        self.runtime.startup_probe_pending_result = false;
                        match result {
                            SavePollResult::Saved { path, .. } => {
                                tracing::info!(
                                    "Startup recorder probe clip available at {}",
                                    path.display()
                                );
                                tasks.push(self.inspect_and_delete_startup_probe(path));
                            }
                            SavePollResult::SaveFailed(error) => {
                                tracing::warn!("Startup recorder probe save failed: {error}");
                            }
                            SavePollResult::BackendEvent(_) => {}
                        }
                        continue;
                    }

                    match result {
                        SavePollResult::Saved {
                            path,
                            duration,
                            audio_layout,
                        } => {
                            tracing::info!("Saved clip available at {}", path.display());
                            if self.config.recorder.clip_saved_notifications {
                                self.notifications.notify_clip_saved(duration);
                            }
                            tasks.push(self.record_save_outcome(PendingSaveOutcome::Saved {
                                path,
                                duration,
                                audio_layout,
                            }));
                        }
                        SavePollResult::SaveFailed(error) => {
                            self.set_clip_error(error.clone());
                            tracing::error!("Recorder save failed: {error}");
                            tasks.push(self.record_save_outcome(PendingSaveOutcome::Failed));
                        }
                        SavePollResult::BackendEvent(event) => {
                            if self.apply_backend_runtime_event(event) {
                                tasks.push(self.sync_tray_snapshot());
                            }
                        }
                    }
                }

                let hotkey_events = self.runtime.hotkeys.drain_events();
                if !hotkey_events.is_empty() {
                    tracing::debug!(
                        event_count = hotkey_events.len(),
                        ?hotkey_events,
                        "runtime poll received manual clip hotkey events"
                    );
                }
                for event in hotkey_events {
                    match event {
                        HotkeyEvent::Activated => {
                            tracing::debug!("queueing manual clip save from hotkey activation");
                            tasks.push(Task::done(Message::RequestManualClipSave));
                        }
                    }
                }

                if let Some(tray) = &self.runtime.tray {
                    let tray_events = tray.drain_events();
                    for event in tray_events {
                        match event {
                            TrayEvent::StartMonitoring => {
                                tasks.push(Task::done(Message::runtime(
                                    RuntimeMessage::StartMonitoring,
                                )));
                            }
                            TrayEvent::StopMonitoring => {
                                tasks.push(Task::done(Message::runtime(
                                    RuntimeMessage::StopMonitoring,
                                )));
                            }
                            TrayEvent::ShowWindow => {
                                tasks.push(self.show_window_task());
                            }
                            TrayEvent::SwitchProfile(profile_id) => {
                                self.apply_manual_profile_selection(profile_id);
                            }
                            TrayEvent::Quit => {
                                tasks.push(iced::exit());
                            }
                        }
                    }
                }

                if let Some(task) = self.poll_active_clip_capture() {
                    tasks.push(task);
                }

                if let Some(task) = self.poll_startup_probe() {
                    tasks.push(task);
                }

                tasks.push(self.process_background_job_notifications());

                Task::batch(tasks)
            }
            RuntimeMessage::Tick => {
                let mut tasks = Vec::new();
                self.event_log.prune(Utc::now());

                if matches!(
                    self.runtime.lifecycle,
                    AppState::WaitingForGame | AppState::WaitingForLogin
                ) {
                    if let Some(pid) = self.process_watcher.find_running_pid() {
                        let (recorder_ready, recorder_task) = self.ensure_ps2_recorder_running(pid);
                        tasks.push(recorder_task);
                        if !recorder_ready {
                            return Task::batch(tasks);
                        }

                        if !matches!(self.runtime.lifecycle, AppState::WaitingForLogin) {
                            self.runtime.lifecycle = AppState::WaitingForLogin;
                            tracing::info!("PS2 process found (pid {pid})");
                            tasks.push(self.check_online_status());
                            return Task::batch(tasks);
                        }
                    } else {
                        self.stop_recorder_if_running();
                        if matches!(self.runtime.lifecycle, AppState::WaitingForLogin) {
                            self.runtime.lifecycle = AppState::WaitingForGame;
                            self.rule_engine.reset();
                            tracing::info!("PS2 process exited");
                        }
                    }
                } else if matches!(self.runtime.lifecycle, AppState::Monitoring { .. }) {
                    if let Some(pid) = self.process_watcher.find_running_pid() {
                        let (_, recorder_task) = self.ensure_ps2_recorder_running(pid);
                        tasks.push(recorder_task);
                    } else {
                        tracing::info!("PS2 exited while monitoring");
                        self.runtime.lifecycle = AppState::WaitingForGame;
                        self.rule_engine.reset();
                        self.runtime.honu_session_id = None;
                        self.stop_recorder_if_running();
                        tasks.push(self.finish_active_session());
                    }
                }
                tasks.push(Task::none());
                let active_character_id = match &self.runtime.lifecycle {
                    AppState::Monitoring { character_id, .. } => Some(*character_id),
                    _ => None,
                };
                tasks.push(self.evaluate_runtime_auto_switch(Utc::now(), active_character_id));
                if self.should_auto_check_for_updates() {
                    self.updates.state.checking = true;
                    self.updates.state.phase = UpdatePhase::Checking;
                    self.updates.state.progress = Some(UpdateProgressState {
                        detail: "Checking GitHub Releases for a newer version.".into(),
                    });
                    tasks.push(self.check_for_updates_task(false));
                }
                if self.should_auto_apply_staged_update() {
                    tasks.push(Task::done(Message::updates(
                        UpdateMessage::InstallDownloadedUpdateWhenIdle,
                    )));
                }
                Task::batch(tasks)
            }
            RuntimeMessage::RecorderStartCompleted { id } => {
                self.complete_pending_recorder_start(id)
            }
            RuntimeMessage::OnlineStatusChecked(online_ids) => {
                if !matches!(self.runtime.lifecycle, AppState::WaitingForLogin) {
                    return Task::none();
                }
                let Some(&id) = online_ids.first() else {
                    return Task::none();
                };
                let character_name = self
                    .config
                    .characters
                    .iter()
                    .find(|c| c.character_id == Some(id))
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("character {id}"));
                tracing::info!("{character_name} already logged in ({id})");
                self.enter_monitoring(character_name, id)
            }
            RuntimeMessage::HonuSessionResolved(result) => {
                match result {
                    Ok(Some(session_id)) => {
                        tracing::info!("Honu session resolved: {session_id}");
                        self.runtime.honu_session_id = Some(session_id);
                    }
                    Ok(None) => {
                        tracing::info!("No active Honu session found");
                    }
                    Err(error) => {
                        tracing::warn!("Failed to resolve Honu session: {error}");
                    }
                }
                Task::none()
            }
            RuntimeMessage::CensusStream(event) => self.handle_census_stream(event),
            RuntimeMessage::MainWindowOpened(window_id) => {
                self.runtime.main_window_id = Some(window_id);
                Task::none()
            }
            RuntimeMessage::MainWindowClosed(window_id) => {
                if self.runtime.main_window_id == Some(window_id) {
                    self.runtime.main_window_id = None;
                }
                Task::none()
            }
            RuntimeMessage::WindowCloseRequested(window_id) => {
                if self.config.minimize_to_tray && self.runtime.main_window_id == Some(window_id) {
                    self.runtime.main_window_id = None;
                    return window::close(window_id);
                }

                iced::exit()
            }
            #[cfg(not(target_os = "windows"))]
            RuntimeMessage::HotkeysConfigured { generation, result } => {
                if generation != self.runtime.hotkey_config_generation {
                    return Task::none();
                }

                self.finish_hotkey_configuration(result.map(take_hotkey_config_result));
                Task::none()
            }
        }
    }

    pub(super) fn ensure_ps2_recorder_running(&mut self, ps2_pid: u32) -> (bool, Task<Message>) {
        if self.recorder.is_running() {
            return (true, Task::none());
        }

        if self.recorder.backend_id() == "obs" && self.runtime.obs_restart_requires_manual_restart {
            return (false, Task::none());
        }

        let capture_plan = if self.recorder.backend_id() == "obs" {
            process::CaptureSourcePlan {
                target: process::CaptureTarget::BackendOwned,
                backend_hints: process::BackendHints::default(),
            }
        } else {
            match self
                .process_watcher
                .resolve_capture_target(ps2_pid, &self.config.recorder.gsr().capture_source)
            {
                Ok(capture_plan) => capture_plan,
                Err(error) => {
                    tracing::debug!(
                        "Waiting for PlanetSide 2 window before starting recorder: {error}"
                    );
                    return (false, Task::none());
                }
            }
        };

        if self
            .runtime
            .pending_recorder_start
            .as_ref()
            .is_some_and(|pending| pending.capture_plan == capture_plan)
        {
            return (false, Task::none());
        }

        (false, self.start_recorder_in_background(capture_plan))
    }

    fn start_recorder_in_background(
        &mut self,
        capture_plan: process::CaptureSourcePlan,
    ) -> Task<Message> {
        self.cancel_pending_recorder_start();

        let start_id = self.runtime.next_recorder_start_id;
        self.runtime.next_recorder_start_id += 1;
        let backend = self.recorder.backend_handle();
        let request = self.recorder.capture_request(&capture_plan);
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let task = Task::perform(
            async move {
                let result = tokio::task::spawn_blocking(move || backend.spawn_replay(request))
                    .await
                    .map_err(|error| {
                        crate::capture::CaptureError::SpawnFailed(format!(
                            "failed to join recorder startup worker: {error}"
                        ))
                    })
                    .and_then(|result| result);
                let _ = result_tx.send(result);
            },
            move |_| Message::runtime(RuntimeMessage::RecorderStartCompleted { id: start_id }),
        );
        let (task, abort_handle) = task.abortable();

        self.runtime.pending_recorder_start = Some(PendingRecorderStart {
            id: start_id,
            capture_plan,
            result_rx,
            abort_handle,
        });

        task
    }

    pub(in crate::app) fn cancel_pending_recorder_start(&mut self) {
        if let Some(pending) = self.runtime.pending_recorder_start.take() {
            pending.abort_handle.abort();
        }
    }

    pub(super) fn complete_pending_recorder_start(&mut self, id: u64) -> Task<Message> {
        let Some(pending_id) = self
            .runtime
            .pending_recorder_start
            .as_ref()
            .map(|pending| pending.id)
        else {
            return Task::none();
        };
        if pending_id != id {
            return Task::none();
        }

        let pending = self
            .runtime
            .pending_recorder_start
            .take()
            .expect("pending recorder start vanished unexpectedly");
        let mut result_rx = pending.result_rx;
        let result = result_rx.try_recv().unwrap_or_else(|_| {
            Err(crate::capture::CaptureError::SpawnFailed(
                "recorder startup completed without a result".into(),
            ))
        });

        match result {
            Ok(session) => {
                if !matches!(
                    self.runtime.lifecycle,
                    AppState::WaitingForGame
                        | AppState::WaitingForLogin
                        | AppState::Monitoring { .. }
                ) {
                    return Task::none();
                }

                match self.recorder.attach_session(pending.capture_plan, session) {
                    Ok(()) => {
                        self.runtime.portal_capture_recovery_notified = false;
                        self.runtime.obs_connection_status = None;
                        self.runtime.obs_restart_requires_manual_restart = false;

                        let mut tasks = vec![self.sync_tray_snapshot()];
                        if matches!(self.runtime.lifecycle, AppState::WaitingForGame)
                            && let Some(pid) = self.process_watcher.find_running_pid()
                        {
                            self.runtime.lifecycle = AppState::WaitingForLogin;
                            tracing::info!("PS2 process found (pid {pid})");
                            tasks.push(self.check_online_status());
                        }
                        Task::batch(tasks)
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Recorder finished starting but could not attach session: {error}"
                        );
                        Task::none()
                    }
                }
            }
            Err(error) => {
                let error_text = error.to_string();
                tracing::error!("Failed to start recorder: {error_text}");

                if self.recorder.backend_id() == "obs" {
                    let status = capture::ObsConnectionStatus::Failed {
                        reason: error_text.clone(),
                    };
                    let changed = self.runtime.obs_connection_status.as_ref() != Some(&status);
                    self.runtime.obs_connection_status = Some(status);
                    self.runtime.obs_restart_requires_manual_restart = true;
                    if changed {
                        self.set_status_feedback(
                            format!(
                                "OBS failed to start monitoring: {error_text}. Fix OBS and restart monitoring."
                            ),
                            false,
                        );
                    }
                    return self.sync_tray_snapshot();
                }

                self.set_status_feedback(format!("Failed to start recorder: {error_text}"), false);
                Task::none()
            }
        }
    }

    pub(super) fn stop_recorder_if_running(&mut self) {
        self.cancel_pending_recorder_start();
        self.runtime.active_clip_capture = None;
        self.runtime.startup_probe_due_at = None;
        self.runtime.startup_probe_pending_result = false;
        self.runtime.startup_probe_resolution = None;
        self.runtime.obs_connection_status = None;
        self.runtime.obs_restart_requires_manual_restart = false;
        if !self.recorder.is_running() {
            return;
        }

        if let Err(error) = self.recorder.stop() {
            tracing::warn!("Failed to stop recorder: {error}");
        }
    }

    pub(super) fn enter_monitoring(
        &mut self,
        character_name: String,
        character_id: u64,
    ) -> Task<Message> {
        if self.notifications_enabled() {
            self.notifications
                .notify_character_confirmed(character_name.as_str());
        }
        let started_at = Utc::now();
        self.event_log.clear();
        self.runtime.honu_session_id = None;
        self.runtime.tracked_alerts.clear();
        self.runtime.manual_profile_override_profile_id = None;
        self.runtime.last_auto_switch_rule_id = None;
        self.runtime.active_session = Some(MonitoringSession {
            id: format!("{character_id}-{}", started_at.timestamp_millis()),
            started_at,
            character_id,
            character_name: character_name.clone(),
        });
        self.runtime.lifecycle = AppState::Monitoring {
            character_name,
            character_id,
        };
        self.rule_engine.reset();
        self.runtime.startup_probe_due_at = Some(Instant::now() + Duration::from_secs(6));
        self.runtime.startup_probe_pending_result = false;
        self.runtime.startup_probe_resolution = None;
        Task::batch([
            self.fetch_honu_session(character_id),
            self.evaluate_runtime_auto_switch(Utc::now(), Some(character_id)),
            self.sync_tray_snapshot(),
        ])
    }
}

pub(super) fn initial_runtime_state(config: &Config) -> AppState {
    if config.auto_start_monitoring {
        AppState::WaitingForGame
    } else {
        AppState::Idle
    }
}
