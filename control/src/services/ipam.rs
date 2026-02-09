use anyhow::Result;
use std::sync::atomic::{AtomicU8, Ordering};

/// Simple IPAM for allocating /24 subnets from 10.42.0.0/16
pub struct SimpleIPAM {
    next_subnet: AtomicU8,
}

impl SimpleIPAM {
    pub fn new() -> Self {
        Self {
            next_subnet: AtomicU8::new(1), // Start at 10.42.1.0/24 (skip .0 for safety)
        }
    }

    /// Allocate the next available /24 subnet
    pub fn allocate_subnet(&self) -> Result<String> {
        let subnet_id = self.next_subnet.fetch_add(1, Ordering::Relaxed);

        if subnet_id > 255 {
            anyhow::bail!("Exhausted subnet pool (max 255 nodes)");
        }

        Ok(format!("10.42.{}.0/24", subnet_id))
    }

    /// Initialize IPAM by finding the highest allocated subnet from database
    pub fn init_from_db(max_subnet_id: u8) -> Self {
        Self {
            next_subnet: AtomicU8::new(max_subnet_id + 1),
        }
    }
}

impl Default for SimpleIPAM {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_allocation() {
        let ipam = SimpleIPAM::new();

        assert_eq!(ipam.allocate_subnet().unwrap(), "10.42.1.0/24");
        assert_eq!(ipam.allocate_subnet().unwrap(), "10.42.2.0/24");
        assert_eq!(ipam.allocate_subnet().unwrap(), "10.42.3.0/24");
    }

    #[test]
    fn test_init_from_db() {
        let ipam = SimpleIPAM::init_from_db(10);
        assert_eq!(ipam.allocate_subnet().unwrap(), "10.42.11.0/24");
    }
}
