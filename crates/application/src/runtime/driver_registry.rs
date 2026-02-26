use domain::connection::Connection;
use domain::driver::{DriverConnection, DriverType};
use std::collections::HashMap;

pub trait DriverFactory: Send + Sync {
    fn create(&self, connection: &Connection) -> Box<dyn DriverConnection>;
}

pub struct DriverRegistry {
    factories: HashMap<DriverType, Box<dyn DriverFactory>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register(&mut self, driver_type: DriverType, factory: Box<dyn DriverFactory>) {
        self.factories.insert(driver_type, factory);
    }

    pub fn create_driver(&self, connection: &Connection) -> Option<Box<dyn DriverConnection>> {
        self.factories
            .get(&connection.config.driver_type)
            .map(|f| f.create(connection))
    }
}
