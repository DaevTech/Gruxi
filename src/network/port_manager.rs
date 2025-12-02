use log::{debug, info, trace, warn};
use std::sync::Arc;
use std::{collections::HashMap, sync::OnceLock};
use tokio::sync::Mutex;

static PORT_MANAGER_SINGLETON: OnceLock<PortManager> = OnceLock::new();

pub fn get_port_manager() -> &'static PortManager {
    PORT_MANAGER_SINGLETON.get_or_init(|| PortManager::new(9000, 10000))
}

/// A generalized port manager that assigns unique ports to processes
/// and allows reuse when processes are stopped.
///
/// Features:
/// - Thread-safe port allocation and deallocation
/// - Automatic port reuse when processes stop
/// - Singleton pattern - only one instance exists globally
/// - Port range: 9000-10000
/// - Support for multiple service types
#[derive(Clone)]
pub struct PortManager {
    inner: Arc<Mutex<PortManagerInner>>,
}

struct PortManagerInner {
    /// Starting port number for allocation
    start_port: u16,
    /// Maximum port number for allocation
    max_port: u16,
    /// Currently allocated ports with their assigned process/service IDs
    allocated_ports: HashMap<u16, String>,
    /// Available ports that can be reused
    available_ports: Vec<u16>,
    /// Next port to try for allocation
    next_port: u16,
}

impl PortManager {
    /// Create a new port manager with the specified port range
    /// Note: Consider using `instance()` for the singleton instead
    ///
    /// # Arguments
    /// * `start_port` - The starting port number (inclusive)
    /// * `max_port` - The maximum port number (inclusive)
    pub fn new(start_port: u16, max_port: u16) -> Self {
        PortManager {
            inner: Arc::new(Mutex::new(PortManagerInner {
                start_port,
                max_port,
                allocated_ports: HashMap::new(),
                available_ports: Vec::new(),
                next_port: start_port,
            })),
        }
    }

    /// Allocate a port for the specified service/process ID
    ///
    /// # Arguments
    /// * `service_id` - Unique identifier for the service/process requesting the port
    ///
    /// # Returns
    /// * `Some(port)` - If a port was successfully allocated
    /// * `None` - If no ports are available
    pub async fn allocate_port(&self, service_id: String) -> Option<u16> {
        let mut inner = self.inner.lock().await;

        // First, try to reuse an available port
        if let Some(port) = inner.available_ports.pop() {
            inner.allocated_ports.insert(port, service_id.clone());
            info!("Allocated reused port {} to service '{}'", port, service_id);
            return Some(port);
        }

        // If no available ports, try to allocate a new one
        let start_search = inner.next_port;
        loop {
            let port = inner.next_port;

            // Check if we've exceeded the maximum port
            if port > inner.max_port {
                inner.next_port = inner.start_port;
            } else {
                inner.next_port += 1;
            }

            // Check if this port is not already allocated
            if !inner.allocated_ports.contains_key(&port) && port <= inner.max_port {
                inner.allocated_ports.insert(port, service_id.clone());
                debug!("Allocated new port {} to service '{}'", port, service_id);
                return Some(port);
            }

            // If we've wrapped around and checked all ports, no ports available
            if inner.next_port == start_search || (port > inner.max_port && inner.next_port == inner.start_port) {
                warn!("No available ports for service '{}'", service_id);
                return None;
            }
        }
    }

    /// Release a port, making it available for reuse
    ///
    /// # Arguments
    /// * `port` - The port number to release
    pub async fn release_port(&self, port: u16) {
        let mut inner = self.inner.lock().await;

        if let Some(service_id) = inner.allocated_ports.remove(&port) {
            inner.available_ports.push(port);
            trace!("Released port {} from service '{}'", port, service_id);
            trace!("Available ports: {:?}", inner.available_ports);
        } else {
            warn!("Attempted to release port {} which was not allocated", port);
        }
    }

    /// Release all ports for a specific service ID
    ///
    /// # Arguments
    /// * `service_id` - The service ID to release all ports for
    pub async fn release_all_ports_for_service(&self, service_id: &str) -> Vec<u16> {
        let mut inner = self.inner.lock().await;
        let mut released_ports = Vec::new();

        // Find all ports allocated to this service
        let ports_to_release: Vec<u16> = inner.allocated_ports.iter().filter(|(_, sid)| sid.as_str() == service_id).map(|(port, _)| *port).collect();

        // Release each port
        for port in ports_to_release {
            inner.allocated_ports.remove(&port);
            inner.available_ports.push(port);
            released_ports.push(port);
        }

        if !released_ports.is_empty() {
            info!("Released {} ports from service '{}': {:?}", released_ports.len(), service_id, released_ports);
        }

        released_ports
    }

    /// Get information about currently allocated ports
    pub async fn get_allocation_info(&self) -> HashMap<u16, String> {
        let inner = self.inner.lock().await;
        inner.allocated_ports.clone()
    }

    /// Get the count of available ports
    pub async fn available_port_count(&self) -> usize {
        let inner = self.inner.lock().await;
        let total_range = (inner.max_port - inner.start_port + 1) as usize;
        let allocated_count = inner.allocated_ports.len();
        total_range - allocated_count
    }
}

#[tokio::test]
async fn test_port_allocation() {
    let manager = PortManager::new(9000, 9002);

    // Test basic allocation
    let port1 = manager.allocate_port("service1".to_string()).await;
    assert_eq!(port1, Some(9000));

    let port2 = manager.allocate_port("service2".to_string()).await;
    assert_eq!(port2, Some(9001));

    let port3 = manager.allocate_port("service3".to_string()).await;
    assert_eq!(port3, Some(9002));

    // Should return None when no more ports available
    let port4 = manager.allocate_port("service4".to_string()).await;
    assert_eq!(port4, None);
}

#[tokio::test]
async fn test_port_release_and_reuse() {
    let manager = PortManager::new(9000, 9001);

    // Allocate all ports
    let port1 = manager.allocate_port("service1".to_string()).await;
    let port2 = manager.allocate_port("service2".to_string()).await;
    assert_eq!(port1, Some(9000));
    assert_eq!(port2, Some(9001));

    // No more ports available
    let port3 = manager.allocate_port("service3".to_string()).await;
    assert_eq!(port3, None);

    // Release a port
    manager.release_port(9000).await;

    // Should be able to reuse the released port
    let port4 = manager.allocate_port("service4".to_string()).await;
    assert_eq!(port4, Some(9000));
}

#[tokio::test]
async fn test_release_all_ports_for_service() {
    let manager = PortManager::new(9000, 9002);

    // Allocate ports to different services
    manager.allocate_port("service1".to_string()).await;
    manager.allocate_port("service1".to_string()).await;
    manager.allocate_port("service2".to_string()).await;

    // Release all ports for service1
    let released = manager.release_all_ports_for_service("service1").await;
    assert_eq!(released.len(), 2);

    // Should be able to allocate new ports now
    let port = manager.allocate_port("service3".to_string()).await;
    assert!(port.is_some());
}

#[tokio::test]
async fn test_singleton_manager() {
    let manager = get_port_manager();

    let port = manager.allocate_port("php-worker-1".to_string()).await;
    let port = port.expect("expected a port to be allocated");
    assert!(port >= 9000 && port <= 10000);

    let available_count = manager.available_port_count().await;
    assert!(available_count >= 1 && available_count <= 1000);
}
