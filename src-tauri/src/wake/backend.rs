use super::{WakeBackend, WakeOptions};

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct SystemWakeBackend {
        child: Mutex<Option<Child>>,
    }

    impl SystemWakeBackend {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl WakeBackend for SystemWakeBackend {
        fn supported(&self) -> bool {
            true
        }

        fn platform(&self) -> &'static str {
            "macos"
        }

        fn acquire(&self, options: WakeOptions) -> Result<(), String> {
            let mut child = self
                .child
                .lock()
                .map_err(|_| "wake backend lock poisoned".to_string())?;
            if child.is_some() {
                return Err("wake assertion is already active".to_string());
            }

            let mut command = Command::new("/usr/bin/caffeinate");
            command.arg("-i");
            if options.keep_display_awake {
                command.arg("-d");
            }
            command
                .arg("-w")
                .arg(std::process::id().to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            *child = Some(
                command
                    .spawn()
                    .map_err(|error| format!("failed to start /usr/bin/caffeinate: {error}"))?,
            );
            Ok(())
        }

        fn release(&self) -> Result<(), String> {
            let mut slot = self
                .child
                .lock()
                .map_err(|_| "wake backend lock poisoned".to_string())?;
            let Some(mut child) = slot.take() else {
                return Ok(());
            };
            match child
                .try_wait()
                .map_err(|error| format!("failed to query caffeinate: {error}"))?
            {
                Some(_) => Ok(()),
                None => {
                    child
                        .kill()
                        .map_err(|error| format!("failed to stop caffeinate: {error}"))?;
                    child
                        .wait()
                        .map_err(|error| format!("failed to reap caffeinate: {error}"))?;
                    Ok(())
                }
            }
        }
    }

    impl Drop for SystemWakeBackend {
        fn drop(&mut self) {
            let _ = self.release();
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired,
        PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows_sys::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
    use windows_sys::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };

    #[derive(Default)]
    pub struct SystemWakeBackend {
        handle: Mutex<Option<usize>>,
    }

    impl SystemWakeBackend {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl WakeBackend for SystemWakeBackend {
        fn supported(&self) -> bool {
            true
        }

        fn platform(&self) -> &'static str {
            "windows"
        }

        fn acquire(&self, options: WakeOptions) -> Result<(), String> {
            let mut slot = self
                .handle
                .lock()
                .map_err(|_| "wake backend lock poisoned".to_string())?;
            if slot.is_some() {
                return Err("wake assertion is already active".to_string());
            }

            let mut reason: Vec<u16> = "AgentGate AI task running"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let context = REASON_CONTEXT {
                Version: POWER_REQUEST_CONTEXT_VERSION,
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 {
                    SimpleReasonString: reason.as_mut_ptr(),
                },
            };
            let handle = unsafe { PowerCreateRequest(&context) };
            if handle.is_null() {
                return Err(format!(
                    "PowerCreateRequest failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            if unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) } == 0 {
                unsafe { CloseHandle(handle) };
                return Err(format!(
                    "PowerSetRequest(SystemRequired) failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if options.keep_display_awake
                && unsafe { PowerSetRequest(handle, PowerRequestDisplayRequired) } == 0
            {
                unsafe {
                    PowerClearRequest(handle, PowerRequestSystemRequired);
                    CloseHandle(handle);
                }
                return Err(format!(
                    "PowerSetRequest(DisplayRequired) failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            *slot = Some(handle as usize);
            Ok(())
        }

        fn release(&self) -> Result<(), String> {
            let mut slot = self
                .handle
                .lock()
                .map_err(|_| "wake backend lock poisoned".to_string())?;
            let Some(handle_value) = slot.take() else {
                return Ok(());
            };
            let handle = handle_value as HANDLE;
            unsafe {
                PowerClearRequest(handle, PowerRequestDisplayRequired);
                PowerClearRequest(handle, PowerRequestSystemRequired);
                CloseHandle(handle);
            }
            Ok(())
        }
    }

    impl Drop for SystemWakeBackend {
        fn drop(&mut self) {
            let _ = self.release();
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;

    #[derive(Default)]
    pub struct SystemWakeBackend;

    impl SystemWakeBackend {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl WakeBackend for SystemWakeBackend {
        fn supported(&self) -> bool {
            false
        }

        fn platform(&self) -> &'static str {
            std::env::consts::OS
        }

        fn acquire(&self, _options: WakeOptions) -> Result<(), String> {
            Err(format!(
                "system wake management is not supported on {}",
                std::env::consts::OS
            ))
        }

        fn release(&self) -> Result<(), String> {
            Ok(())
        }
    }
}

pub use platform::SystemWakeBackend;
