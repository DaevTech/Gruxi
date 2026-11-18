use crate::core::database_connection::get_database_connection;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{DatabaseError, GruxiErrorKind, ManagedSystemErrorKind};
use crate::external_connections::managed_system::php_cgi::PhpCgi;
use crate::external_connections::managed_systems_handler::ManagedSystemsHandler;
use tokio::sync::{Mutex, OnceCell};

pub struct ManagedSystemsManager {
    handler: ManagedSystemsHandler,
}

impl ManagedSystemsManager {
    pub fn new() -> Self {
        Self {
            handler: ManagedSystemsHandler::new(),
        }
    }

    pub async fn initialize(&mut self, systems: &[PhpCgi]) {
        for system in systems {
            if let Err(error) = self.handler.start_managed_system(system).await {
                crate::error!("Failed to start managed system {}: {:?}", system.id, error);
            }
        }
    }

    pub fn get_configuration_managed_systems(&self) -> Result<Vec<PhpCgi>, GruxiError> {
        let connection = get_database_connection().map_err(|_| GruxiError::new_with_kind_only(GruxiErrorKind::Database(DatabaseError::ConnectionFailed)))?;
        self.load_managed_systems_from_db(&connection)
            .map_err(|error| GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), format!("Failed to load managed systems from database: {}", error)))
    }

    pub fn get_port_for_php_cgi(&self, id: &str) -> Result<u16, GruxiError> {
        self.handler.get_port_for_php_cgi(id)
    }

    pub fn get_connection_semaphore(&self, id: &str) -> Option<std::sync::Arc<tokio::sync::Semaphore>> {
        self.handler.get_connection_semaphore(id)
    }

    pub async fn create_managed_system(&mut self, system: &PhpCgi) -> Result<(), GruxiError> {
        let connection = get_database_connection().map_err(|_| GruxiError::new_with_kind_only(GruxiErrorKind::Database(DatabaseError::ConnectionFailed)))?;
        if self
            .managed_system_exists(&connection, &system.id)
            .map_err(|error| database_query_error("Failed to check managed system", error))?
        {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::Conflict)));
        }
        self.insert_managed_system(&connection, system)
            .map_err(|error| database_query_error("Failed to create managed system", error))?;
        if let Err(start_error) = self.handler.start_managed_system(system).await {
            if let Err(cleanup_error) = self.delete_managed_system_from_db(&connection, &system.id) {
                return Err(GruxiError::new(
                    GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StartFailed),
                    format!("Failed to start managed system: {:?}; failed to remove its saved configuration: {}", start_error, cleanup_error),
                ));
            }
            return Err(start_error);
        }
        Ok(())
    }

    pub async fn update_managed_system(&mut self, system: &PhpCgi) -> Result<(), GruxiError> {
        let connection = get_database_connection().map_err(|_| GruxiError::new_with_kind_only(GruxiErrorKind::Database(DatabaseError::ConnectionFailed)))?;
        let Some(previous) = self
            .load_managed_system_from_db(&connection, &system.id)
            .map_err(|error| database_query_error("Failed to load managed system", error))?
        else {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::NotFound)));
        };

        self.handler.stop_managed_system(&system.id).await?;
        if let Err(update_error) = self.update_managed_system_in_db(&connection, system) {
            return match self.handler.start_managed_system(&previous).await {
                Ok(()) => Err(database_query_error("Failed to update managed system", update_error)),
                Err(start_error) => Err(GruxiError::new(
                    GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StartFailed),
                    format!("Failed to update managed system: {}; failed to restart its process: {:?}", update_error, start_error),
                )),
            };
        }

        if let Err(start_error) = self.handler.start_managed_system(system).await {
            let restore_config = self.update_managed_system_in_db(&connection, &previous);
            let restore_process = self.handler.start_managed_system(&previous).await;
            let detail = match (restore_config, restore_process) {
                (Ok(()), Ok(())) => "The previous configuration and process were restored.".to_string(),
                (Err(config_error), Ok(())) => format!("The previous process was restarted, but restoring its configuration failed: {}", config_error),
                (Ok(()), Err(process_error)) => format!("The previous configuration was restored, but its process failed to restart: {:?}", process_error),
                (Err(config_error), Err(process_error)) => format!("Restoring the previous configuration failed: {}; restarting its process failed: {:?}", config_error, process_error),
            };
            return Err(GruxiError::new(
                GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StartFailed),
                format!("Failed to start updated managed system: {:?}. {}", start_error, detail),
            ));
        }
        Ok(())
    }

    pub async fn delete_managed_system(&mut self, id: &str) -> Result<(), GruxiError> {
        let connection = get_database_connection().map_err(|_| GruxiError::new_with_kind_only(GruxiErrorKind::Database(DatabaseError::ConnectionFailed)))?;
        let Some(previous) = self
            .load_managed_system_from_db(&connection, id)
            .map_err(|error| database_query_error("Failed to load managed system", error))?
        else {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::NotFound)));
        };

        self.handler.stop_managed_system(id).await?;
        if let Err(delete_error) = self.delete_managed_system_from_db(&connection, id) {
            return match self.handler.start_managed_system(&previous).await {
                Ok(()) => Err(database_query_error("Failed to delete managed system", delete_error)),
                Err(start_error) => Err(GruxiError::new(
                    GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StartFailed),
                    format!("Failed to delete managed system: {}; failed to restart its process: {:?}", delete_error, start_error),
                )),
            };
        }
        Ok(())
    }

    fn load_managed_systems_from_db(&self, connection: &sqlite::Connection) -> Result<Vec<PhpCgi>, sqlite::Error> {
        let mut statement = connection.prepare("SELECT id, name, request_timeout, concurrent_threads, executable FROM php_cgi_handlers ORDER BY id")?;
        let mut systems = Vec::new();
        while let sqlite::State::Row = statement.next()? {
            let id = statement.read(0)?;
            let name = statement.read(1)?;
            let request_timeout = statement.read::<i64, _>(2)? as u32;
            let concurrent_threads = statement.read::<i64, _>(3)? as u32;
            let executable = statement.read(4)?;
            systems.push(PhpCgi::new(id, name, request_timeout, concurrent_threads, executable));
        }
        Ok(systems)
    }

    fn load_managed_system_from_db(&self, connection: &sqlite::Connection, id: &str) -> Result<Option<PhpCgi>, sqlite::Error> {
        let mut statement = connection.prepare("SELECT name, request_timeout, concurrent_threads, executable FROM php_cgi_handlers WHERE id = ?")?;
        statement.bind((1, id))?;
        if let sqlite::State::Row = statement.next()? {
            let name = statement.read(0)?;
            let request_timeout = statement.read::<i64, _>(1)? as u32;
            let concurrent_threads = statement.read::<i64, _>(2)? as u32;
            let executable = statement.read(3)?;
            Ok(Some(PhpCgi::new(id.to_string(), name, request_timeout, concurrent_threads, executable)))
        } else {
            Ok(None)
        }
    }

    fn managed_system_exists(&self, connection: &sqlite::Connection, id: &str) -> Result<bool, sqlite::Error> {
        let mut statement = connection.prepare("SELECT EXISTS(SELECT 1 FROM php_cgi_handlers WHERE id = ?)")?;
        statement.bind((1, id))?;
        statement.next()?;
        Ok(statement.read::<i64, _>(0)? != 0)
    }

    fn insert_managed_system(&self, connection: &sqlite::Connection, system: &PhpCgi) -> Result<(), sqlite::Error> {
        let mut statement = connection.prepare("INSERT INTO php_cgi_handlers (id, name, request_timeout, concurrent_threads, executable) VALUES (?, ?, ?, ?, ?)")?;
        statement.bind((1, system.id.as_str()))?;
        statement.bind((2, system.name.as_str()))?;
        statement.bind((3, system.request_timeout as i64))?;
        statement.bind((4, system.concurrent_threads as i64))?;
        statement.bind((5, system.executable.as_str()))?;
        statement.next()?;
        Ok(())
    }

    fn update_managed_system_in_db(&self, connection: &sqlite::Connection, system: &PhpCgi) -> Result<(), sqlite::Error> {
        let mut statement = connection.prepare("UPDATE php_cgi_handlers SET name = ?, request_timeout = ?, concurrent_threads = ?, executable = ? WHERE id = ?")?;
        statement.bind((1, system.name.as_str()))?;
        statement.bind((2, system.request_timeout as i64))?;
        statement.bind((3, system.concurrent_threads as i64))?;
        statement.bind((4, system.executable.as_str()))?;
        statement.bind((5, system.id.as_str()))?;
        statement.next()?;
        Ok(())
    }

    fn delete_managed_system_from_db(&self, connection: &sqlite::Connection, id: &str) -> Result<(), sqlite::Error> {
        let mut statement = connection.prepare("DELETE FROM php_cgi_handlers WHERE id = ?")?;
        statement.bind((1, id))?;
        statement.next()?;
        Ok(())
    }
}

fn database_query_error(context: &str, error: sqlite::Error) -> GruxiError {
    GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), format!("{}: {}", context, error))
}

static MANAGED_SYSTEMS_MANAGER: OnceCell<Mutex<ManagedSystemsManager>> = OnceCell::const_new();

pub async fn get_managed_systems_manager() -> &'static Mutex<ManagedSystemsManager> {
    MANAGED_SYSTEMS_MANAGER
        .get_or_init(|| async {
            let mut manager = ManagedSystemsManager::new();
            let configuration = crate::config::cached_configuration::get_cached_configuration().get_configuration();
            manager.initialize(&configuration.php_cgi_handlers).await;
            Mutex::new(manager)
        })
        .await
}
