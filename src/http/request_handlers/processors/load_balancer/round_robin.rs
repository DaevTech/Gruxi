use std::{collections::HashMap, sync::atomic::{AtomicI32, Ordering}};
use crate::http::request_handlers::processors::load_balancer::load_balancer::LoadBalancerTrait;

pub struct RoundRobin {
    pub servers: Vec<String>,
    pub current_index: AtomicI32,
    pub health_state: HashMap<String, bool>,
}

impl RoundRobin {
    pub fn new(servers: Vec<String>) -> Self {
        // We assume all servers are healthy by default
        let health_state = servers.iter().map(|s| (s.clone(), true)).collect();

        Self {
            servers,
            current_index: AtomicI32::new(0),
            health_state,
        }
    }
}

impl LoadBalancerTrait for RoundRobin {
    fn get_next_server(&self) -> Option<String> {
        let total_servers = self.servers.len();
        if total_servers == 0 {
            return None;
        }

        for _ in 0..total_servers {
            let server = &self.servers[self.current_index.load(Ordering::Relaxed) as usize];
            self.current_index.store((self.current_index.load(Ordering::Relaxed) + 1) % total_servers as i32, Ordering::Relaxed);

            // If the server is healthy, return it, otherwise continue to the next
            if *self.health_state.get(server).unwrap_or(&true) {
                return Some(server.clone());
            }
        }

        // If none of the servers are healthy, return None
        None
    }

    fn check_health(&mut self) {
        // Placeholder for health check logic
        for server in &self.servers {
            // Here you would implement actual health check logic
            // For now, we assume all servers are healthy
            self.health_state.insert(server.clone(), true);
        }
    }
}