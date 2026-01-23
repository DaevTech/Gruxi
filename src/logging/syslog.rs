use chrono::Utc;
use std::fmt;
use std::sync::{LazyLock, RwLock};
use tokio::select;

use crate::core::operation_mode::OperationMode;
use crate::logging::buffered_log::BufferedLog;

pub struct SysLog {
    pub buffered_log: BufferedLog,
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
            buffered_log: BufferedLog::new("syslog".to_string(), "./logs/gruxi.log".to_string()),
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
    }

    pub fn start_flushing_task(&self) {
        tokio::spawn(Self::start_flushing_thread());
    }

    pub fn add_log(&self, log_type: LogType, log: String) {
        // Match the logtype against the enabled levels
        match log_type {
            LogType::Error if !self.error_enabled => return,
            LogType::Warn if !self.warn_enabled => return,
            LogType::Info if !self.info_enabled => return,
            LogType::Debug if !self.debug_enabled => return,
            LogType::Trace if !self.trace_enabled => return,
            _ => {}
        }

        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        let log_entry = format!("{} - [{}] {}", &ts, &log_type, &log);

        // Also print to stdout right away if enabled
        match log_type {
            LogType::Error if self.stdout_error_enabled => println!("{}", &log_entry),
            LogType::Warn if self.stdout_warn_enabled => println!("{}", &log_entry),
            LogType::Info if self.stdout_info_enabled => println!("{}", &log_entry),
            LogType::Debug if self.stdout_debug_enabled => println!("{}", &log_entry),
            LogType::Trace if self.stdout_trace_enabled => println!("{}", &log_entry),
            _ => {}
        }

        self.buffered_log.buffered_log.lock().unwrap().push(log_entry);
    }

    pub async fn start_flushing_thread() {
        let triggers = crate::core::triggers::get_trigger_handler();
        let mut operation_mode_changed_token = triggers
            .get_trigger("operation_mode_changed")
            .expect("Failed to get operation_mode_changed trigger")
            .read()
            .await
            .clone();
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();

        loop {
            select! {
                // Ideally, this would be adjustable according to the work load (such as elapsed time to do a flush in average)
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    SYS_LOG.read().unwrap().buffered_log.consider_flush(false);
                },
                _ = operation_mode_changed_token.cancelled() => {
                    // Get new operation mode
                    let operation_mode = crate::core::operation_mode::get_operation_mode();
                    let new_log_level = Self::get_log_level_based_on_operation_mode(operation_mode);
                    SysLog::set_new_log_level(new_log_level);
                    // Get new token for next time
                    operation_mode_changed_token = triggers
                        .get_trigger("operation_mode_changed")
                        .expect("Failed to get operation_mode_changed trigger")
                        .read()
                        .await
                        .clone();

                },
                _ = shutdown_token.cancelled() => {
                    // Shutdown in progress, we force flush the logs
                    let sys_log = SYS_LOG.read().unwrap();
                    sys_log.buffered_log.consider_flush(true);
                    break;
                },
            }
        }
    }

    fn set_new_log_level(new_log_level: LogType) {
        SYS_LOG.write().unwrap().buffered_log.consider_flush(true);
        SYS_LOG.write().unwrap().log_level = new_log_level;
        SYS_LOG.write().unwrap().calculate_enabled_levels();
    }

    pub fn set_new_stdout_log_level(new_log_level: LogType) {
        SYS_LOG.write().unwrap().stdout_log_level = new_log_level;
        SYS_LOG.write().unwrap().calculate_enabled_levels();
    }

    fn get_log_level_based_on_operation_mode(operation_mode: OperationMode) -> LogType {
        match operation_mode {
            OperationMode::DEV => LogType::Trace,
            OperationMode::DEBUG => LogType::Debug,
            OperationMode::PRODUCTION => LogType::Info,
            OperationMode::ULTIMATE => LogType::Warn,
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
        OperationMode::ULTIMATE => LogType::Warn,
    };

    let sys_log = SysLog::new(log_level, LogType::Info);
    sys_log.start_flushing_task();
    sys_log
}

pub fn error<S: Into<String>>(log: S) {
    SYS_LOG.read().unwrap().add_log(LogType::Error, log.into());
}

pub fn warn<S: Into<String>>(log: S) {
    SYS_LOG.read().unwrap().add_log(LogType::Warn, log.into());
}

pub fn info<S: Into<String>>(log: S) {
    SYS_LOG.read().unwrap().add_log(LogType::Info, log.into());
}

pub fn debug<S: Into<String>>(log: S) {
    SYS_LOG.read().unwrap().add_log(LogType::Debug, log.into());
}

pub fn trace<S: Into<String>>(log: S) {
    SYS_LOG.read().unwrap().add_log(LogType::Trace, log.into());
}
