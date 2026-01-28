use crate::http::request_handlers::processors::load_balancer::load_balancer::LoadBalancerImpl;

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Simple round-robin load balancer
pub struct RoundRobin {
    servers: Vec<String>,
    current_index: usize,
    health_state: HashMap<String, Arc<AtomicBool>>,
    health_url_path: String,
    health_timeout_secs: u64,
    health_check_interval_secs: u64,
}

impl RoundRobin {
    pub fn new(servers: Vec<String>, health_url_path: String, health_timeout_secs: u64, health_check_interval_secs: u64) -> Self {
        // All servers are healthy at start
        let health_state = servers.iter().map(|s| (s.clone(), Arc::new(AtomicBool::new(true)))).collect();

        Self {
            servers,
            current_index: 0,
            health_state,
            health_url_path,
            health_timeout_secs,
            health_check_interval_secs,
        }
    }
}

impl LoadBalancerImpl for RoundRobin {
    fn get_next_server(&mut self) -> Option<String> {
        let total = self.servers.len();
        if total == 0 {
            return None;
        }

        for _ in 0..total {
            let server = &self.servers[self.current_index];
            self.current_index = (self.current_index + 1) % total;

            match self.health_state.get(server) {
                None => continue,
                Some(health) => {
                    if health.load(Ordering::SeqCst) {
                        return Some(server.clone());
                    }
                }
            }
        }

        None
    }

    fn check_health(&mut self) {
        for server in &self.servers {
            let server_uri = server.clone() + &self.health_url_path;
            let healthy_state_option = self.health_state.get(server);
            let healthy_state = match healthy_state_option {
                Some(s) => s.clone(),
                None => continue,
            };
            self.check_uri_health(&server_uri, healthy_state, self.health_timeout_secs);
        }
    }

    fn get_health_check_interval_secs(&self) -> u64 {
        self.health_check_interval_secs
    }
}
