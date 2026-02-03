use chrono::Utc;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use tokio::select;

use crate::core::operation_mode::OperationMode;
use crate::logging::buffered_log::BufferedLog;

// Atomic flags for fast lock-free log level checks
// Initialize to true so logs work before SYS_LOG LazyLock is initialized
static ERROR_ENABLED: AtomicBool = AtomicBool::new(true);
static WARN_ENABLED: AtomicBool = AtomicBool::new(true);
static INFO_ENABLED: AtomicBool = AtomicBool::new(true);
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(true);
static TRACE_ENABLED: AtomicBool = AtomicBool::new(true);

pub struct SysLog {
    pub buffered_log: Arc<BufferedLog>,
    // Log level for writing log
    log_level: LogType,
    // Enabled levels for both logs
    error_enabled: bool,
    info_enabled: bool,
    warn_enabled: bool,
    debug_enabled: bool,
    trace_enabled: bool,
    // Log level for stdout
    stdout_log_level: LogType,
    // Enabled levels for stdout
    stdout_error_enabled: bool,
    stdout_info_enabled: bool,
    stdout_warn_enabled: bool,
    stdout_debug_enabled: bool,
    stdout_trace_enabled: bool,
}

#[repr(u8)]
#[derive(Clone, Debug)]
pub enum LogType {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogType::Error => write!(f, "ERROR"),
            LogType::Info => write!(f, "INFO"),
            LogType::Warn => write!(f, "WARN"),
            LogType::Debug => write!(f, "DEBUG"),
            LogType::Trace => write!(f, "TRACE"),
            _ => write!(f, "UNKNOWN"),
        }
    }
}

impl SysLog {
    pub fn new(log_level: LogType, stdout_log_level: LogType) -> Self {
        let mut sys_log = SysLog {
            buffered_log: Arc::new(BufferedLog::new("./logs/gruxi.log".to_string(), 1000000)),
            log_level: log_level.clone(),
            error_enabled: false,
            info_enabled: false,
            warn_enabled: false,
            debug_enabled: false,
            trace_enabled: false,
            stdout_log_level: stdout_log_level.clone(),
            stdout_error_enabled: false,
            stdout_info_enabled: false,
            stdout_warn_enabled: false,
            stdout_debug_enabled: false,
            stdout_trace_enabled: false,
        };

        sys_log.calculate_enabled_levels();

        sys_log
    }

    pub fn calculate_enabled_levels(&mut self) {
        let log_level = self.log_level.clone();
        let stdout_log_level = self.stdout_log_level.clone();
        // Log file levels enabled
        self.error_enabled = log_level.clone() as u8 >= LogType::Error as u8;
        self.warn_enabled = log_level.clone() as u8 >= LogType::Warn as u8;
        self.info_enabled = log_level.clone() as u8 >= LogType::Info as u8;
        self.debug_enabled = log_level.clone() as u8 >= LogType::Debug as u8;
        self.trace_enabled = log_level.clone() as u8 >= LogType::Trace as u8;
        // Stdout levels enabled
        self.stdout_error_enabled = stdout_log_level.clone() as u8 >= LogType::Error as u8;
        self.stdout_warn_enabled = stdout_log_level.clone() as u8 >= LogType::Warn as u8;
        self.stdout_info_enabled = stdout_log_level.clone() as u8 >= LogType::Info as u8;
        self.stdout_debug_enabled = stdout_log_level.clone() as u8 >= LogType::Debug as u8;
        self.stdout_trace_enabled = stdout_log_level.clone() as u8 >= LogType::Trace as u8;

        // Update the atomic flags for lock-free checks (combine file and stdout)
        ERROR_ENABLED.store(self.error_enabled || self.stdout_error_enabled, Ordering::Relaxed);
        WARN_ENABLED.store(self.warn_enabled || self.stdout_warn_enabled, Ordering::Relaxed);
        INFO_ENABLED.store(self.info_enabled || self.stdout_info_enabled, Ordering::Relaxed);
        DEBUG_ENABLED.store(self.debug_enabled || self.stdout_debug_enabled, Ordering::Relaxed);
        TRACE_ENABLED.store(self.trace_enabled || self.stdout_trace_enabled, Ordering::Relaxed);
    }

    pub fn start_flushing_task(&self) {
        tokio::spawn(Self::start_flushing_thread());
    }

    pub fn add_log(&self, log_type: LogType, log: String) {
        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

        let log_entry = match tokio::task::try_id() {
            Some(task_id) => {
             format!("{} - [{}][ID:{}] {}", &ts, &log_type, task_id, &log)
            }
            None => {
                format!("{} - [{}] {}", &ts, &log_type, &log)
            }
        };

        // Print to stdout if enabled for this level
        match log_type {
            LogType::Error if self.stdout_error_enabled => println!("{}", &log_entry),
            LogType::Warn if self.stdout_warn_enabled => println!("{}", &log_entry),
            LogType::Info if self.stdout_info_enabled => println!("{}", &log_entry),
            LogType::Debug if self.stdout_debug_enabled => println!("{}", &log_entry),
            LogType::Trace if self.stdout_trace_enabled => println!("{}", &log_entry),
            _ => {}
        }

        // Write to file if enabled for this level
        let file_enabled = match log_type {
            LogType::Error => self.error_enabled,
            LogType::Warn => self.warn_enabled,
            LogType::Info => self.info_enabled,
            LogType::Debug => self.debug_enabled,
            LogType::Trace => self.trace_enabled,
            LogType::Off => false,
        };

        if file_enabled {
            self.buffered_log.add_log(log_entry);
        }
    }

    pub async fn start_flushing_thread() {
        let triggers = crate::core::triggers::get_trigger_handler();

        let operation_mode_changed_token_option = triggers.get_token("operation_mode_changed").await;
        let mut operation_mode_changed_token = match operation_mode_changed_token_option {
            Some(token) => token,
            None => {
                _log_error("Failed to get operation_mode_changed token - Could not start flushing thread for syslog. Please report a bug".to_string());
                return;
            }
        };

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                _log_error("Failed to get shutdown token - Could not start flushing thread for syslog. Please report a bug".to_string());
                return;
            }
        };

        loop {
            select! {
                // Ideally, this would be adjustable according to the work load (such as elapsed time to do a flush in average)
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    let buffered_log = match SYS_LOG.read() {
                        Err(_) => {
                            // Can't log here - would need the lock we failed to acquire
                            continue;
                        },
                        Ok(sys_log) => sys_log.buffered_log.clone(),
                    };
                    buffered_log.flush(false).await;
                },
                _ = operation_mode_changed_token.cancelled() => {
                    // Get new operation mode
                    let operation_mode = crate::core::operation_mode::get_operation_mode();
                    let new_log_level = Self::get_log_level_based_on_operation_mode(operation_mode);
                    SysLog::set_new_log_level(new_log_level).await;

                    // Get new token for next time
                    let operation_mode_changed_token_option = triggers.get_token("operation_mode_changed").await;
                    operation_mode_changed_token = match operation_mode_changed_token_option {
                        Some(token) => token,
                        None => {
                            _log_error("Failed to get operation_mode_changed token - Could not start flushing thread for syslog. Please report a bug".to_string());
                            return;
                        }
                    };

                },
                _ = shutdown_token.cancelled() => {
                    // Shutdown in progress, we force flush the logs
                    let buffered_log = match SYS_LOG.read() {
                        Err(_) => {
                            // Can't log here - would need the lock we failed to acquire
                            break;
                        },
                        Ok(sys_log) => sys_log.buffered_log.clone(),
                    };
                    buffered_log.flush(true).await;
                    break;
                },
            }
        }
    }

    async fn set_new_log_level(new_log_level: LogType) {
        let buffered_log = match SYS_LOG.write() {
            Err(_) => {
                // Can't log - we failed to get the write lock for the logger itself
                eprintln!("Failed to acquire write lock for syslog when setting new log level");
                return;
            }
            Ok(mut guard) => {
                guard.log_level = new_log_level;
                guard.calculate_enabled_levels();
                guard.buffered_log.clone()
            }
        };
        // Flush after releasing the write lock
        buffered_log.flush(true).await;
    }

    pub fn set_new_stdout_log_level(new_log_level: LogType) {
        match SYS_LOG.write() {
            Err(_) => {
                eprintln!("Failed to acquire write lock for syslog when setting new stdout log level");
                return;
            }
            Ok(mut guard) => {
                guard.stdout_log_level = new_log_level;
                guard.calculate_enabled_levels();
            }
        }
    }

    fn get_log_level_based_on_operation_mode(operation_mode: OperationMode) -> LogType {
        match operation_mode {
            OperationMode::DEV => LogType::Trace,
            OperationMode::DEBUG => LogType::Debug,
            OperationMode::PRODUCTION => LogType::Info,
            OperationMode::ULTIMATE => LogType::Error,
        }
    }
}

pub static SYS_LOG: LazyLock<RwLock<SysLog>> = LazyLock::new(|| RwLock::new(init_log()));

fn init_log() -> SysLog {
    // Get operation mode
    let operation_mode = crate::core::operation_mode::get_operation_mode();

    // Determine log level
    let log_level = match operation_mode {
        OperationMode::DEV => LogType::Trace,
        OperationMode::DEBUG => LogType::Debug,
        OperationMode::PRODUCTION => LogType::Info,
        OperationMode::ULTIMATE => LogType::Error,
    };

    let sys_log = SysLog::new(log_level, LogType::Info);
    sys_log.start_flushing_task();
    sys_log
}

// Check functions that return whether a log level is enabled
// These use atomic loads - no locking required
#[inline]
pub fn is_error_enabled() -> bool {
    ERROR_ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn is_warn_enabled() -> bool {
    WARN_ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn is_info_enabled() -> bool {
    INFO_ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn is_trace_enabled() -> bool {
    TRACE_ENABLED.load(Ordering::Relaxed)
}

// Internal functions used by macros - these assume the level check has already been done
#[doc(hidden)]
pub fn _log_error(log: String) {
    if let Ok(sys_log) = SYS_LOG.read() {
        sys_log.add_log(LogType::Error, log);
    }
}

#[doc(hidden)]
pub fn _log_warn(log: String) {
    if let Ok(sys_log) = SYS_LOG.read() {
        sys_log.add_log(LogType::Warn, log);
    }
}

#[doc(hidden)]
pub fn _log_info(log: String) {
    if let Ok(sys_log) = SYS_LOG.read() {
        sys_log.add_log(LogType::Info, log);
    }
}

#[doc(hidden)]
pub fn _log_debug(log: String) {
    if let Ok(sys_log) = SYS_LOG.read() {
        sys_log.add_log(LogType::Debug, log);
    }
}

#[doc(hidden)]
pub fn _log_trace(log: String) {
    if let Ok(sys_log) = SYS_LOG.read() {
        sys_log.add_log(LogType::Trace, log);
    }
}

/// Logs an error message. The format arguments are only evaluated if error logging is enabled.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        if $crate::logging::syslog::is_error_enabled() {
            $crate::logging::syslog::_log_error(format!($($arg)*));
        }
    };
}

/// Logs a warning message. The format arguments are only evaluated if warn logging is enabled.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        if $crate::logging::syslog::is_warn_enabled() {
            $crate::logging::syslog::_log_warn(format!($($arg)*));
        }
    };
}

/// Logs an info message. The format arguments are only evaluated if info logging is enabled.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if $crate::logging::syslog::is_info_enabled() {
            $crate::logging::syslog::_log_info(format!($($arg)*));
        }
    };
}

/// Logs a debug message. The format arguments are only evaluated if debug logging is enabled.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::logging::syslog::is_debug_enabled() {
            $crate::logging::syslog::_log_debug(format!($($arg)*));
        }
    };
}

/// Logs a trace message. The format arguments are only evaluated if trace logging is enabled.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::logging::syslog::is_trace_enabled() {
            $crate::logging::syslog::_log_trace(format!($($arg)*));
        }
    };
}
