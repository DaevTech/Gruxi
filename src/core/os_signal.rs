use tokio::signal;
use crate::core::shutdown_manager::get_shutdown_manager;
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
                handle_graceful_shutdown();
            }
        } => {},
        _ = async {
            loop {
                sigint.recv().await;
                handle_graceful_shutdown();
            }
        } => {},
        _ = async {
            loop {
                sighup.recv().await;
                handle_configuration_reload();
            }
        } => {},
    }
}

#[cfg(windows)]
async fn handle_windows_signals() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        signal::ctrl_c().await?;
        handle_graceful_shutdown();
    }
}

pub fn start_os_signal_handling() {
    #[cfg(unix)]
    tokio::spawn(async {
        if let Err(e) = handle_unix_signals().await {
            log::error!("Error handling Unix signals: {}", e);
        }
    });

    #[cfg(windows)]
    tokio::spawn(async {
        if let Err(e) = handle_windows_signals().await {
            log::error!("Error handling Windows signals: {}", e);
        }
    });
}

// Do graceful shutdown
fn handle_graceful_shutdown() {
    log::info!("Starting shutting down Grux...");
    get_shutdown_manager().initiate_shutdown();

}

// Configuration reload due to OS signals
fn handle_configuration_reload() {
    log::info!("Reloading configuration...");
    // Set a flag or notify relevant components to reload configuration
}
