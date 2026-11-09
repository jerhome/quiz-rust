use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use quiz_core::{Question, Quiz, QuizConfig, QuizState};
use quiz_llm::{ApiKey, ClaudeClient, LlmError};
use quiz_net::{
    Client, ClientHandle, DiscoveryHandle, Host, HostConfig, HostHandle, NetError, Waker,
};
use tokio::runtime::Runtime;

use crate::i18n::{tf, Key, Msg};
use crate::multi::{self, MPhase, Session};
use crate::screens::{self, join::JoinState, setup::SetupForm};
use crate::settings::{self, Settings};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Solo,
    Host,
    Join,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Settings,
    Join,
    HostStarting,
    Lobby,
    MultiQuestion,
    MultiReveal,
    MultiEnd,
    Setup,
    Generating,
    GenerationError,
    Question,
    Reveal,
    Score,
}

pub fn screen_for(state: QuizState, has_error: bool) -> Screen {
    match state {
        QuizState::Setup if has_error => Screen::GenerationError,
        QuizState::Setup => Screen::Setup,
        QuizState::Generating => Screen::Generating,
        QuizState::Question { .. } => Screen::Question,
        QuizState::Reveal { .. } => Screen::Reveal,
        QuizState::Finished => Screen::Score,
    }
}

pub fn screen_for_route(
    route: Route,
    state: QuizState,
    has_error: bool,
    host_starting: bool,
    configuring: bool,
    phase: Option<MPhase>,
) -> Screen {
    if configuring && phase == Some(MPhase::Lobby) {
        return screen_for(state, has_error);
    }
    if let Some(p) = phase {
        return match p {
            MPhase::Lobby => Screen::Lobby,
            MPhase::Question => Screen::MultiQuestion,
            MPhase::Reveal => Screen::MultiReveal,
            MPhase::End => Screen::MultiEnd,
        };
    }
    match route {
        Route::Home => Screen::Home,
        Route::Join => Screen::Join,
        Route::Settings => Screen::Settings,
        Route::Host if host_starting => Screen::HostStarting,
        Route::Host | Route::Solo => screen_for(state, has_error),
    }
}

fn unusable(e: &quiz_core::QuizError) -> Msg {
    Msg::from_fn(|l| tf(l, Key::ErrUnusable, &[&e.message(l)]))
}

fn waker(ctx: &egui::Context) -> Waker {
    let ctx = ctx.clone();
    std::sync::Arc::new(move || ctx.request_repaint())
}

type Tx<T> = Sender<Result<T, NetError>>;
type Rx<T> = Receiver<Result<T, NetError>>;

pub struct QuizApp {
    pub route: Route,
    pub notice: Option<Msg>,
    pub session: Option<Session>,
    pub host_starting: bool,
    pub configuring: bool,
    pub join: JoinState,
    pending_questions: Vec<Question>,
    host_chan: (Tx<HostHandle>, Rx<HostHandle>),
    join_chan: (Tx<ClientHandle>, Rx<ClientHandle>),
    disc_chan: (Tx<DiscoveryHandle>, Rx<DiscoveryHandle>),

    pub quiz: Quiz,
    pub form: SetupForm,
    pub gen_error: Option<Msg>,
    runtime: Runtime,
    tx: Sender<Result<Vec<Question>, LlmError>>,
    rx: Receiver<Result<Vec<Question>, LlmError>>,
    api_key: String,
    pub settings: Settings,
    pub model_other: bool,
    pub model_text: String,
    pub save_failed: bool,
    settings_path: Option<std::path::PathBuf>,
    saved: Settings,
    dirty_since: Option<Instant>,
    env_key: Option<String>,
    pub confirm_quit: bool,
    confirm_restart: bool,
    pub restart_failed: bool,
    /// Selection for which the user chose "Later" (banner hidden until the choice changes).
    pub restart_dismissed: Option<crate::backend::Backend>,
    pub active_backend: crate::backend::Active,
    quitting: bool,
}

impl QuizApp {
    pub fn new(
        ctx: &egui::Context,
        runtime: Runtime,
        active_backend: crate::backend::Active,
    ) -> Self {
        crate::theme::install(ctx);
        let (tx, rx) = channel();
        let settings_path = settings::config_path();
        let settings = settings_path
            .as_deref()
            .map(settings::load_from)
            .unwrap_or_default();
        let env_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let key = settings::initial_key(env_key.as_deref(), settings.api_key.as_deref());
        let model_other = !quiz_llm::MODELS.iter().any(|(id, _)| *id == settings.model);
        Self {
            model_other,
            model_text: if model_other {
                settings.model.clone()
            } else {
                String::new()
            },
            save_failed: false,
            settings_path,
            saved: settings.clone(),
            dirty_since: None,
            env_key,
            confirm_quit: false,
            confirm_restart: false,
            restart_failed: false,
            restart_dismissed: None,
            active_backend,
            quitting: false,
            route: Route::Home,
            notice: None,
            session: None,
            host_starting: false,
            configuring: false,
            join: JoinState::default(),
            pending_questions: Vec::new(),
            host_chan: channel(),
            join_chan: channel(),
            disc_chan: channel(),
            quiz: Quiz::new(QuizConfig::default()),
            form: SetupForm::from_settings(&settings, key),
            gen_error: None,
            runtime,
            tx,
            rx,
            api_key: String::new(),
            settings,
        }
    }

    pub fn submit_setup(&mut self, ctx: &egui::Context) {
        let config = self.form.config(self.settings.quiz_lang);
        if self.route == Route::Host {
            if let Err(e) = multi::validate_name(&self.form.name) {
                self.form.error = Some(e);
                return;
            }
        }
        if self.form.api_key.trim().is_empty() {
            self.form.error = Some(Msg::Key(Key::ErrKeyRequired));
            return;
        }
        let result = self
            .quiz
            .set_config(config)
            .and_then(|()| self.quiz.start_generation());
        if let Err(e) = result {
            self.form.error = Some(Msg::from_fn(|l| tf(l, Key::ErrConfig, &[&e.message(l)])));
            return;
        }
        self.form.error = None;
        self.api_key = self.form.api_key.clone();
        self.spawn_generation(ctx);
    }

    pub fn retry(&mut self, ctx: &egui::Context) {
        self.gen_error = None;
        if self.route == Route::Host && !self.pending_questions.is_empty() {
            // Host start failed: reuse the questions already generated instead of regenerating.
            self.host_starting = true;
            self.spawn_host(ctx);
        } else if self.quiz.start_generation().is_ok() {
            self.spawn_generation(ctx);
        }
    }

    pub fn back_to_setup(&mut self) {
        self.gen_error = None;
        self.host_starting = false;
    }

    pub fn leave_setup(&mut self) {
        if self.session.is_some() {
            self.configuring = false;
            self.gen_error = None;
            self.quiz = Quiz::new(self.quiz.config().clone());
        } else {
            self.go_home(None);
        }
    }

    pub fn configure_next_game(&mut self) {
        self.configuring = true;
    }

    pub fn go_home(&mut self, notice: Option<Msg>) {
        self.session = None;
        self.join = JoinState::default();
        self.host_starting = false;
        self.configuring = false;
        self.pending_questions.clear();
        self.gen_error = None;
        self.quiz = Quiz::new(self.quiz.config().clone());
        self.route = Route::Home;
        self.notice = notice;
    }

    pub fn enter(&mut self, route: Route, ctx: &egui::Context) {
        self.go_home(None);
        self.route = route;
        if route == Route::Join {
            self.start_discovery(ctx);
        }
    }

    fn begin_host(&mut self, questions: Vec<Question>, ctx: &egui::Context) {
        if let Err(e) = self.quiz.questions_ready(questions.clone()) {
            self.fail(unusable(&e));
            return;
        }
        self.quiz = Quiz::new(self.quiz.config().clone());
        if let Some(session) = self.session.as_mut() {
            session.set_questions(questions, self.form.time_limit_duration());
            self.configuring = false;
            return;
        }
        self.pending_questions = questions;
        self.host_starting = true;
        self.spawn_host(ctx);
    }

    fn spawn_host(&self, ctx: &egui::Context) {
        let cfg = HostConfig {
            name: self.form.name.trim().to_string(),
            time_limit: self.form.time_limit_duration(),
            waker: Some(waker(ctx)),
            ..HostConfig::default()
        };
        let tx = self.host_chan.0.clone();
        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(Host::start(cfg).await);
            ctx.request_repaint();
        });
    }

    pub fn start_discovery(&self, ctx: &egui::Context) {
        let w = waker(ctx);
        let tx = self.disc_chan.0.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(quiz_net::discover(Some(w)).await);
        });
    }

    pub fn connect(&mut self, ctx: &egui::Context, addr: std::net::SocketAddr) {
        let name = match multi::validate_name(&self.form.name) {
            Ok(n) => n,
            Err(e) => {
                self.join.error = Some(e);
                return;
            }
        };
        self.join.error = None;
        self.join.connecting = true;
        let w = waker(ctx);
        let tx = self.join_chan.0.clone();
        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(Client::connect(addr, &name, Some(w)).await);
            ctx.request_repaint();
        });
    }

    fn poll_net(&mut self) {
        while let Ok(r) = self.host_chan.1.try_recv() {
            if !(self.route == Route::Host && self.host_starting) {
                // Cancelled in the meantime: dropping the handle shuts it down.
                continue;
            }
            match r {
                Ok(handle) => {
                    let name = self.form.name.trim().to_string();
                    let questions = std::mem::take(&mut self.pending_questions);
                    self.session = Some(Session::new_host(
                        handle,
                        &name,
                        questions,
                        self.form.time_limit_duration(),
                    ));
                    self.host_starting = false;
                }
                Err(e) => {
                    self.host_starting = false;
                    self.gen_error =
                        Some(Msg::from_fn(|l| tf(l, Key::ErrHostStart, &[&e.message(l)])));
                }
            }
        }
        while let Ok(r) = self.join_chan.1.try_recv() {
            if !(self.route == Route::Join && self.join.connecting) {
                continue;
            }
            self.join.connecting = false;
            match r {
                Ok(handle) => {
                    self.session = Some(Session::new_client(handle));
                    self.join.discovery = None;
                }
                Err(e) => self.join.error = Some(Msg::from_fn(|l| e.message(l))),
            }
        }
        while let Ok(r) = self.disc_chan.1.try_recv() {
            if self.route != Route::Join {
                continue;
            }
            match r {
                Ok(handle) => self.join.discovery = Some(handle),
                Err(e) => {
                    self.join.error =
                        Some(Msg::from_fn(|l| tf(l, Key::ErrDiscovery, &[&e.message(l)])));
                }
            }
        }
        self.join.drain_hosts(Instant::now());
        if let Some(msg) = self.session.as_mut().and_then(Session::drain) {
            self.go_home(Some(msg));
        }
    }

    fn spawn_generation(&self, ctx: &egui::Context) {
        let config = self.quiz.config().clone();
        let key = ApiKey::new(self.api_key.clone());
        let model = self.settings.model.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            let result = match ClaudeClient::new(key, model) {
                Ok(client) => client.generate_quiz(&config).await,
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_generation(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.rx.try_recv() {
            if self.quiz.state() != QuizState::Generating {
                continue;
            }
            match result {
                Ok(questions) if self.route == Route::Host => self.begin_host(questions, ctx),
                Ok(questions) => {
                    if let Err(e) = self.quiz.questions_ready(questions) {
                        self.fail(unusable(&e));
                    }
                }
                Err(e) => self.fail(Msg::from_fn(|l| e.message(l))),
            }
        }
    }

    fn handle_quit_inputs(&mut self, ctx: &egui::Context) {
        let ctrl_q = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Q));
        if ctrl_q && !self.confirm_quit {
            self.request_quit(ctx);
        }
        // Window close button: cancel the close and ask first if something would be lost.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.quit_kind().is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_quit = true;
            } else {
                self.quitting = true;
                self.cleanup();
            }
        }
    }

    fn show_quit_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_quit {
            return;
        }
        let Some(kind) = self.quit_kind() else {
            self.confirm_quit = false;
            return;
        };
        let restart = self.confirm_restart;
        match crate::chrome::confirm_modal(ctx, kind, self.lang(), restart) {
            Some(true) => {
                self.confirm_quit = false;
                self.confirm_restart = false;
                if restart {
                    self.restart_now(ctx);
                } else {
                    self.finish_quit(ctx);
                }
            }
            Some(false) => {
                self.confirm_quit = false;
                self.confirm_restart = false;
            }
            None => {}
        }
    }

    pub fn lang(&self) -> quiz_core::Lang {
        self.settings.ui_lang
    }

    fn snapshot(&self) -> Settings {
        let mut s = self.settings.clone();
        s.difficulty = self.form.difficulty;
        s.count = self.form.count;
        s.time_limit_secs = self.form.time_limit.unwrap_or(0);
        s.name = self.form.name.trim().to_string();
        s.selected_themes = self.form.picker.catalog_ids();
        s.selected_custom = self.form.picker.selected_custom();
        s.custom_themes = self.form.picker.custom.clone();
        let key = self.form.api_key.trim();
        s.api_key = settings::key_to_persist(
            s.remember_key,
            key,
            self.env_key.as_deref(),
            self.saved.api_key.as_deref(),
        );
        s
    }

    fn persist(&mut self, ctx: &egui::Context) {
        let snap = self.snapshot();
        if snap == self.saved {
            self.dirty_since = None;
            return;
        }
        let since = *self.dirty_since.get_or_insert_with(Instant::now);
        // Debounce: while a text field has focus, wait up to 800 ms before writing to disk.
        if ctx.wants_keyboard_input() && since.elapsed() < Duration::from_millis(800) {
            ctx.request_repaint_after(Duration::from_millis(300));
            return;
        }
        self.write_settings(snap);
    }

    fn write_settings(&mut self, snap: Settings) {
        if let Some(path) = &self.settings_path {
            self.save_failed = settings::save_to(path, &snap).is_err();
            if self.save_failed {
                tracing::warn!("enregistrement des réglages impossible");
            }
        }
        self.saved = snap;
        self.dirty_since = None;
    }

    fn flush_settings(&mut self) {
        let snap = self.snapshot();
        if snap != self.saved {
            self.write_settings(snap);
        }
    }

    fn cleanup(&mut self) {
        self.flush_settings();
        // Dropping the session calls shutdown() (host) / disconnect() (client).
        self.session = None;
        self.join.discovery = None;
    }

    pub fn restart_needed(&self) -> bool {
        crate::backend::restart_needed(self.settings.display_backend, self.active_backend)
    }

    /// Asks for confirmation first when a quiz or session would be lost.
    pub fn request_restart(&mut self, ctx: &egui::Context) {
        self.restart_failed = false;
        if self.quit_kind().is_some() {
            self.confirm_quit = true;
            self.confirm_restart = true;
        } else {
            self.restart_now(ctx);
        }
    }

    // Settings must be on disk before the new instance starts, since it reads them at launch.
    // If the launch fails the app (and any session) stays open.
    fn restart_now(&mut self, ctx: &egui::Context) {
        self.flush_settings();
        match crate::backend::spawn_relaunch(None) {
            Ok(()) => self.finish_quit(ctx),
            Err(_) => self.restart_failed = true,
        }
    }

    fn quit_kind(&self) -> Option<crate::chrome::QuitKind> {
        crate::chrome::quit_confirmation(
            self.quiz.state(),
            self.session.as_ref().map(Session::is_host),
        )
    }

    pub fn request_quit(&mut self, ctx: &egui::Context) {
        if self.quit_kind().is_some() {
            self.confirm_quit = true;
        } else {
            self.finish_quit(ctx);
        }
    }

    fn finish_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.cleanup();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn key_from_env(&self) -> bool {
        settings::key_is_from_env(&self.form.api_key, self.env_key.as_deref())
    }

    pub fn forget_key(&mut self) {
        self.settings.remember_key = false;
        self.settings.api_key = None;
        self.form.api_key = self.env_key.clone().unwrap_or_default();
    }

    fn fail(&mut self, message: Msg) {
        tracing::warn!("génération échouée");
        let _ = self.quiz.generation_failed();
        self.gen_error = Some(message);
    }
}

impl eframe::App for QuizApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cleanup();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_generation(ctx);
        self.poll_net();
        self.persist(ctx);
        self.handle_quit_inputs(ctx);
        let screen = screen_for_route(
            self.route,
            self.quiz.state(),
            self.gen_error.is_some(),
            self.host_starting,
            self.configuring,
            self.session.as_ref().map(|s| s.phase),
        );
        if screen == Screen::Join {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        crate::chrome::header(ctx, self, screen);
        self.show_quit_confirmation(ctx);
        let frame = egui::Frame::NONE.fill(crate::theme::BG);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            crate::widgets::page(ui, |ui| match screen {
                Screen::Home => screens::home::show(ui, self),
                Screen::Join => screens::join::show(ui, self),
                Screen::Settings => screens::settings::show(ui, self),
                Screen::HostStarting => {
                    screens::generating::show_spinner(ui, Key::Opening, self.lang())
                }
                Screen::Lobby => screens::lobby::show(ui, self),
                Screen::MultiQuestion => screens::multi::show_question(ui, self),
                Screen::MultiReveal => screens::multi::show_reveal(ui, self),
                Screen::MultiEnd => screens::multi::show_end(ui, self),
                Screen::Setup => screens::setup::show(ui, self),
                Screen::Generating => {
                    screens::generating::show_spinner(ui, Key::Generating, self.lang())
                }
                Screen::GenerationError => screens::generating::show_error(ui, self),
                Screen::Question => screens::question::show(ui, self),
                Screen::Reveal => screens::reveal::show(ui, self),
                Screen::Score => screens::score::show(ui, self),
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_route_ecran() {
        let st = QuizState::Setup;
        let f = |r, p| screen_for_route(r, st, false, false, false, p);
        assert_eq!(f(Route::Home, None), Screen::Home);
        assert_eq!(f(Route::Join, None), Screen::Join);
        assert_eq!(f(Route::Settings, None), Screen::Settings);
        assert_eq!(f(Route::Solo, None), Screen::Setup);
        assert_eq!(f(Route::Host, None), Screen::Setup);
        assert_eq!(f(Route::Join, Some(MPhase::Lobby)), Screen::Lobby);
        assert_eq!(f(Route::Host, Some(MPhase::End)), Screen::MultiEnd);
        assert_eq!(
            screen_for_route(Route::Host, st, false, false, true, Some(MPhase::Lobby)),
            Screen::Setup
        );
        assert_eq!(
            screen_for_route(Route::Host, st, false, true, false, None),
            Screen::HostStarting
        );
    }

    #[test]
    fn mapping_etat_ecran() {
        assert_eq!(screen_for(QuizState::Setup, false), Screen::Setup);
        assert_eq!(screen_for(QuizState::Setup, true), Screen::GenerationError);
        assert_eq!(screen_for(QuizState::Generating, false), Screen::Generating);
        assert_eq!(
            screen_for(QuizState::Question { index: 2 }, false),
            Screen::Question
        );
        assert_eq!(
            screen_for(QuizState::Reveal { index: 2 }, false),
            Screen::Reveal
        );
        assert_eq!(screen_for(QuizState::Finished, false), Screen::Score);
    }
}
