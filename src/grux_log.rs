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

// Initilize the logging
pub fn init_logging() -> Result<log4rs::Handle, String> {
    let level = log::LevelFilter::Info;
    let file_path = get_log_location().map_err(|e| format!("Failed to get log location: {}", e))?;
    let file_path = format!("{}/system.log", file_path);

    // Build a stderr logger.
    let stderr = ConsoleAppender::builder().target(Target::Stderr).encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S)(utc)} - {l}: {m}{n})}"))).build();

    // Logging to log file.
    let logfile = FileAppender::builder()
        // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/indeonex.html
        .encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S)(utc)} - {l}: {m}{n})}")))
        .build(file_path)
        .unwrap();

    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    // Trace-only log file
    let trace_logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({d(%d-%m-%Y %H:%M:%S)(utc)} - {l}: {m}{n})}")))
        .build(format!("{}/trace.log", get_log_location().map_err(|e| format!("Failed to get log location: {}", e))?))
        .unwrap();

    let config = Config::builder()
        .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(level))).build("logfile", Box::new(logfile)))
        .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(level))).build("stderr", Box::new(stderr)))
        .appender(Appender::builder()
            .filter(Box::new(ThresholdFilter::new(LevelFilter::Trace)))
            .build("trace", Box::new(trace_logfile)))
        .build(Root::builder()
            .appender("logfile")
            .appender("stderr")
            .appender("trace")
            .build(LevelFilter::Trace))
        .unwrap();

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

    /*

    let exe_path = std::env::current_exe();
    if let Err(e) = exe_path {
        return Err(e.to_string());
    }

    // Get parent dir
    let exe_path = exe_path.unwrap();
    let parent_exe_path = exe_path.parent();
    if let None = parent_exe_path {
        return Err("Failed to get parent directory of executable".to_string());
    }

    // Add the logs directory
    let path = parent_exe_path.unwrap().join("logs");
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(&path) {
            return Err(e.to_string());
        }
    }

    Ok(path.to_str().unwrap().to_string())

    */
}
