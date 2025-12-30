use std::{collections::HashMap, sync::{Arc, RwLock}};

pub trait LoadBalancerTrait: Send + Sync {
    fn get_next_server(&self) -> Option<String>;
    fn check_health(&mut self);
}

pub struct LoadBalancer {
    pub load_balancer: Arc<RwLock<HashMap<String, Arc<RwLock<dyn LoadBalancerTrait>>>>>
}

impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            load_balancer: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_load_balancer(&self, id: &str) -> Option<Arc<RwLock<dyn LoadBalancerTrait>>> {
        self.load_balancer.read().unwrap().get(id).map(|lb| lb.clone())
    }

    pub fn check_load_balancer_exists(&self, id: &str) -> bool {
        self.load_balancer.read().unwrap().contains_key(id)
    }

    pub fn create_load_balancer(&self, id: &str, lb: impl LoadBalancerTrait + 'static) {
        self.load_balancer.write().unwrap().insert(id.to_string(), Arc::new(RwLock::new(lb)));
    }
}
