use crate::core::triggers::get_trigger_handler;
use crate::logging::syslog::{error, info};
#[cfg(windows)]
use tokio::signal;

#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

#[cfg(unix)]
async fn handle_unix_signals() -> Result<(), Box<dyn std::error::Error>> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    tokio::select! {
        _ = async {
            loop {
                sigterm.recv().await;
                let triggers = get_trigger_handler();
                info("Shutdown signal received, starting shutdown process");
                triggers.run_trigger("shutdown").await;
            }
        } => {},
        _ = async {
            loop {
                sigint.recv().await;
                let triggers = get_trigger_handler();
                info("Shutdown signal received, starting shutdown process");
                triggers.run_trigger("shutdown").await;
            }
        } => {},
        _ = async {
            loop {
                sighup.recv().await;
                let triggers = get_trigger_handler();
                info("Reload configuration signal received, starting reload process");
                triggers.run_trigger("reload_configuration").await;
            }
        } => {},
    };

    Ok(())
}

#[cfg(windows)]
async fn handle_windows_signals() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        signal::ctrl_c().await?;
        info("Shutdown signal received, starting shutdown process");
        let triggers = get_trigger_handler();
        triggers.run_trigger("shutdown").await;
    }
}

pub fn start_os_signal_handling() {
    #[cfg(unix)]
    tokio::spawn(async {
        if let Err(e) = handle_unix_signals().await {
            error(format!("Error handling Unix signals: {}", e));
        }
    });

    #[cfg(windows)]
    tokio::spawn(async {
        if let Err(e) = handle_windows_signals().await {
            error(format!("Error handling Windows signals: {}", e));
        }
    });
}
