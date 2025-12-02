use crate::core::operation_mode::OperationMode;
use log::{LevelFilter, SetLoggerError, trace};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
};
use std::fs::OpenOptions;

// Initilize the logging
pub fn init_logging(operation_mode: OperationMode) -> Result<log4rs::Handle, String> {
    let log_path = get_log_location().map_err(|e| format!("Failed to get log location: {}", e))?;

    // Build a stderr logger.
    let stderr = ConsoleAppender::builder()
        .target(Target::Stderr)
        .encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S%.3f)(utc)} - {l}: {m}{n})}")))
        .build();

    // Logging to log file.
    let system_log_file_path = format!("{}/system.log", &log_path);
    let logfile = FileAppender::builder()
        // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/indeonex.html
        .encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S%.3f)(utc)} - {l}: {m}{n})}")))
        .build(&system_log_file_path)
        .map_err(|e| format!("Failed to create system log file '{}/system.log': {}", log_path, e))?;

    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    let trace_logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S%.3f)(utc)} - {l}: {m}{n})}")))
        .build(format!("{}/trace.log", &log_path))
        .map_err(|e| format!("Failed to create trace log file '{}/trace.log': {}", log_path, e))?;

    // Build the log4rs config based on operation mode
    let config = match operation_mode {
        OperationMode::DEV => {
            // We truncate the log files on each start in dev mode
            let _ = OpenOptions::new().write(true).truncate(true).create(true).open(format!("{}/system.log", &log_path)).unwrap();
            let _ = OpenOptions::new().write(true).truncate(true).create(true).open(format!("{}/trace.log", &log_path)).unwrap();

            Config::builder()
                .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Info))).build("logfile", Box::new(logfile)))
                .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Info))).build("stderr", Box::new(stderr)))
                .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Trace))).build("trace", Box::new(trace_logfile)))
                .build(Root::builder().appender("logfile").appender("stderr").appender("trace").build(LevelFilter::Trace))
                .map_err(|e| format!("Failed to build log4rs config for DEV mode: {}", e))?
        }
        OperationMode::DEBUG => Config::builder()
            .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Debug))).build("logfile", Box::new(logfile)))
            .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Info))).build("stdout", Box::new(stderr)))
            .build(Root::builder().appender("logfile").appender("stdout").build(LevelFilter::Debug))
            .map_err(|e| format!("Failed to build log4rs config for DEBUG mode: {}", e))?,
        OperationMode::PRODUCTION => Config::builder()
            .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Info))).build("logfile", Box::new(logfile)))
            .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Info))).build("stderr", Box::new(stderr)))
            .build(Root::builder().appender("logfile").appender("stderr").build(LevelFilter::Info))
            .map_err(|e| format!("Failed to build log4rs config for PRODUCTION mode: {}", e))?,
        OperationMode::SPEEDTEST => Config::builder()
            .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Error))).build("stderr", Box::new(stderr)))
            .build(Root::builder().appender("stderr").build(LevelFilter::Error))
            .map_err(|e| format!("Failed to build log4rs config for SPEEDTEST mode: {}", e))?,
    };

    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let handle = log4rs::init_config(config).map_err(|e: SetLoggerError| e.to_string())?;

    trace!("Logging was started with no problems");

    Ok(handle)
}

fn get_log_location() -> Result<String, String> {
    let log_path = "./logs";
    if !std::path::Path::new(log_path).exists() {
        if let Err(e) = std::fs::create_dir_all(&log_path) {
            return Err(e.to_string());
        }
    }

    Ok(log_path.to_string())
}
