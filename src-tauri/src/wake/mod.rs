use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod backend;
pub use backend::SystemWakeBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeOptions {
    pub keep_display_awake: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct WakeConfig {
    pub enabled: bool,
    pub request_control: bool,
    pub cooldown_seconds: u64,
    pub keep_display_awake: bool,
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_control: false,
            cooldown_seconds: 900,
            keep_display_awake: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WakeMode {
    Unsupported,
    Disabled,
    Continuous,
    Request,
    Cooldown,
    Idle,
    Error,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct WakeStatus {
    pub supported: bool,
    pub platform: String,
    pub enabled: bool,
    pub request_control: bool,
    pub active: bool,
    pub active_requests: u64,
    pub mode: WakeMode,
    pub cooldown_remaining: u64,
    pub elapsed_seconds: u64,
    pub keep_display_awake: bool,
    pub last_error: Option<String>,
}

pub trait WakeBackend: Send + Sync {
    fn supported(&self) -> bool;
    fn platform(&self) -> &'static str;
    fn acquire(&self, options: WakeOptions) -> Result<(), String>;
    fn release(&self) -> Result<(), String>;
}

pub struct WakeManager {
    backend: Arc<dyn WakeBackend>,
    inner: Mutex<WakeInner>,
}

struct WakeInner {
    config: WakeConfig,
    started: bool,
    active: bool,
    active_options: Option<WakeOptions>,
    active_requests: u64,
    activated_at: Option<Instant>,
    cooldown_until: Option<Instant>,
    last_error: Option<String>,
}

impl WakeManager {
    pub fn new() -> Arc<Self> {
        Self::with_backend(Arc::new(SystemWakeBackend::new()))
    }

    pub fn with_backend(backend: Arc<dyn WakeBackend>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            inner: Mutex::new(WakeInner {
                config: WakeConfig::default(),
                started: false,
                active: false,
                active_options: None,
                active_requests: 0,
                activated_at: None,
                cooldown_until: None,
                last_error: None,
            }),
        })
    }

    pub fn start_at(&self, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.started = true;
        self.reconcile(&mut inner, now);
    }

    pub fn set_config_at(&self, config: WakeConfig, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mode_changed = inner.config.enabled != config.enabled
            || inner.config.request_control != config.request_control;
        inner.config = config;
        if mode_changed {
            inner.cooldown_until = None;
        }
        self.reconcile(&mut inner, now);
    }

    pub fn request_started_at(&self, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.active_requests = inner.active_requests.saturating_add(1);
        inner.cooldown_until = None;
        self.reconcile(&mut inner, now);
    }

    pub fn request_finished_at(&self, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.active_requests = inner.active_requests.saturating_sub(1);
        if inner.active_requests == 0 && inner.config.enabled && inner.config.request_control {
            inner.cooldown_until = Some(
                now.checked_add(Duration::from_secs(inner.config.cooldown_seconds))
                    .unwrap_or(now),
            );
        }
        self.reconcile(&mut inner, now);
    }

    pub fn tick_at(&self, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.cooldown_until.is_some_and(|deadline| now >= deadline) {
            inner.cooldown_until = None;
        }
        self.reconcile(&mut inner, now);
    }

    pub fn shutdown_at(&self, _now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.active {
            match self.backend.release() {
                Ok(()) => inner.last_error = None,
                Err(error) => inner.last_error = Some(error),
            }
        }
        inner.started = false;
        inner.active = false;
        inner.active_options = None;
        inner.active_requests = 0;
        inner.activated_at = None;
        inner.cooldown_until = None;
    }

    pub fn start(&self) {
        self.start_at(Instant::now());
    }

    pub fn set_config(&self, config: WakeConfig) {
        self.set_config_at(config, Instant::now());
    }

    pub fn config(&self) -> WakeConfig {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .clone()
    }

    pub fn request_started(&self) {
        self.request_started_at(Instant::now());
    }

    pub fn request_finished(&self) {
        self.request_finished_at(Instant::now());
    }

    pub fn tick(&self) {
        self.tick_at(Instant::now());
    }

    pub fn shutdown(&self) {
        self.shutdown_at(Instant::now());
    }

    pub fn status(&self) -> WakeStatus {
        self.status_at(Instant::now())
    }

    pub fn status_at(&self, now: Instant) -> WakeStatus {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let in_cooldown = inner.cooldown_until.is_some_and(|deadline| now < deadline);
        let cooldown_remaining = inner
            .cooldown_until
            .and_then(|deadline| deadline.checked_duration_since(now))
            .map(|remaining| remaining.as_secs())
            .unwrap_or(0);
        let mode = if !self.backend.supported() {
            WakeMode::Unsupported
        } else if inner.last_error.is_some() {
            WakeMode::Error
        } else if !inner.config.enabled {
            WakeMode::Disabled
        } else if !inner.config.request_control {
            WakeMode::Continuous
        } else if inner.active_requests > 0 {
            WakeMode::Request
        } else if in_cooldown {
            WakeMode::Cooldown
        } else {
            WakeMode::Idle
        };

        WakeStatus {
            supported: self.backend.supported(),
            platform: self.backend.platform().to_string(),
            enabled: inner.config.enabled,
            request_control: inner.config.request_control,
            active: inner.active,
            active_requests: inner.active_requests,
            mode,
            cooldown_remaining,
            elapsed_seconds: inner
                .activated_at
                .map(|started| now.saturating_duration_since(started).as_secs())
                .unwrap_or(0),
            keep_display_awake: inner.config.keep_display_awake,
            last_error: inner.last_error.clone(),
        }
    }

    fn reconcile(&self, inner: &mut WakeInner, now: Instant) {
        let in_cooldown = inner.cooldown_until.is_some_and(|deadline| now < deadline);
        let wanted = inner.started
            && self.backend.supported()
            && inner.config.enabled
            && (!inner.config.request_control || inner.active_requests > 0 || in_cooldown);
        let wanted_options = WakeOptions {
            keep_display_awake: inner.config.keep_display_awake,
        };

        if inner.active && (!wanted || inner.active_options != Some(wanted_options)) {
            match self.backend.release() {
                Ok(()) => {
                    inner.active = false;
                    inner.active_options = None;
                    inner.activated_at = None;
                    inner.last_error = None;
                }
                Err(error) => {
                    inner.last_error = Some(error);
                    return;
                }
            }
        }

        if wanted && !inner.active {
            match self.backend.acquire(wanted_options) {
                Ok(()) => {
                    inner.active = true;
                    inner.active_options = Some(wanted_options);
                    inner.activated_at = Some(now);
                    inner.last_error = None;
                }
                Err(error) => {
                    inner.last_error = Some(error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum BackendCall {
        Acquire(WakeOptions),
        Release,
    }

    struct FakeBackend {
        calls: Mutex<Vec<BackendCall>>,
    }

    impl FakeBackend {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
            })
        }

        fn calls(&self) -> Vec<BackendCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl WakeBackend for FakeBackend {
        fn supported(&self) -> bool {
            true
        }

        fn platform(&self) -> &'static str {
            "test"
        }

        fn acquire(&self, options: WakeOptions) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(BackendCall::Acquire(options));
            Ok(())
        }

        fn release(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push(BackendCall::Release);
            Ok(())
        }
    }

    fn request_config(cooldown_seconds: u64) -> WakeConfig {
        WakeConfig {
            request_control: true,
            cooldown_seconds,
            ..WakeConfig::default()
        }
    }

    #[test]
    fn default_mode_acquires_once_on_startup() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend.clone());
        let now = Instant::now();

        manager.start_at(now);
        manager.start_at(now);

        assert_eq!(
            backend.calls(),
            vec![BackendCall::Acquire(WakeOptions {
                keep_display_awake: false,
            })]
        );
        assert!(manager.status_at(now).active);
        assert_eq!(manager.status_at(now).mode, WakeMode::Continuous);
    }

    #[test]
    fn request_mode_waits_for_all_requests_then_releases_after_cooldown() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend.clone());
        let now = Instant::now();
        manager.set_config_at(request_config(15), now);
        manager.start_at(now);

        manager.request_started_at(now);
        manager.request_started_at(now);
        manager.request_finished_at(now + Duration::from_secs(2));
        assert!(manager.status_at(now + Duration::from_secs(2)).active);
        assert_eq!(manager.status_at(now).active_requests, 1);

        manager.request_finished_at(now + Duration::from_secs(3));
        assert_eq!(
            manager.status_at(now + Duration::from_secs(3)).mode,
            WakeMode::Cooldown
        );
        manager.tick_at(now + Duration::from_secs(17));
        assert!(manager.status_at(now + Duration::from_secs(17)).active);

        manager.tick_at(now + Duration::from_secs(18));
        let status = manager.status_at(now + Duration::from_secs(18));
        assert!(!status.active);
        assert_eq!(status.mode, WakeMode::Idle);
        assert_eq!(
            backend.calls(),
            vec![
                BackendCall::Acquire(WakeOptions {
                    keep_display_awake: false,
                }),
                BackendCall::Release,
            ]
        );
    }

    #[test]
    fn disabling_or_switching_modes_reconciles_immediately() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend.clone());
        let now = Instant::now();
        manager.start_at(now);

        manager.set_config_at(
            WakeConfig {
                enabled: false,
                ..WakeConfig::default()
            },
            now,
        );
        assert!(!manager.status_at(now).active);

        manager.set_config_at(request_config(15), now);
        assert!(!manager.status_at(now).active);
        manager.request_started_at(now);
        assert!(manager.status_at(now).active);

        manager.set_config_at(WakeConfig::default(), now);
        manager.request_finished_at(now);
        manager.set_config_at(request_config(15), now);
        assert!(!manager.status_at(now).active);
        assert_eq!(manager.status_at(now).active_requests, 0);
    }

    #[test]
    fn changing_display_option_replaces_the_active_assertion() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend.clone());
        let now = Instant::now();
        manager.start_at(now);
        manager.set_config_at(
            WakeConfig {
                keep_display_awake: true,
                ..WakeConfig::default()
            },
            now,
        );

        assert_eq!(
            backend.calls(),
            vec![
                BackendCall::Acquire(WakeOptions {
                    keep_display_awake: false,
                }),
                BackendCall::Release,
                BackendCall::Acquire(WakeOptions {
                    keep_display_awake: true,
                }),
            ]
        );
    }

    #[test]
    fn changing_display_option_does_not_cancel_an_active_cooldown() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend);
        let now = Instant::now();
        manager.set_config_at(request_config(15), now);
        manager.start_at(now);
        manager.request_started_at(now);
        manager.request_finished_at(now);

        manager.set_config_at(
            WakeConfig {
                keep_display_awake: true,
                ..request_config(15)
            },
            now + Duration::from_secs(2),
        );

        let status = manager.status_at(now + Duration::from_secs(2));
        assert!(status.active);
        assert_eq!(status.mode, WakeMode::Cooldown);
        assert_eq!(status.cooldown_remaining, 13);
    }

    #[test]
    fn subsecond_cooldown_still_reports_cooldown_mode() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend);
        let now = Instant::now();
        manager.set_config_at(request_config(1), now);
        manager.start_at(now);
        manager.request_started_at(now);
        manager.request_finished_at(now);

        let status = manager.status_at(now + Duration::from_millis(500));
        assert!(status.active);
        assert_eq!(status.cooldown_remaining, 0);
        assert_eq!(status.mode, WakeMode::Cooldown);
    }

    #[test]
    fn shutdown_releases_and_clears_runtime_state() {
        let backend = FakeBackend::new();
        let manager = WakeManager::with_backend(backend.clone());
        let now = Instant::now();
        manager.start_at(now);
        manager.request_started_at(now);

        manager.shutdown_at(now);

        let status = manager.status_at(now);
        assert!(!status.active);
        assert_eq!(status.active_requests, 0);
        assert_eq!(backend.calls().last(), Some(&BackendCall::Release));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn system_backend_can_acquire_and_release_on_macos() {
        let backend = SystemWakeBackend::new();
        assert!(backend.supported());
        backend
            .acquire(WakeOptions {
                keep_display_awake: false,
            })
            .expect("macOS should start a system wake assertion");
        backend
            .release()
            .expect("macOS should release the system wake assertion");
    }
}
