//! Pairing module - sender authentication with pairing codes

use chrono::{Duration, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// Pending pairing request
#[derive(Debug, Clone)]
struct PendingPairing {
    sender_id: String,
    expires_at: chrono::DateTime<Utc>,
}

/// Approved sender
#[derive(Debug, Clone)]
struct ApprovedSender {
    sender_id: String,
    approved_at: chrono::DateTime<Utc>,
}

/// Pairing manager - handles pairing codes for channel access control
pub struct PairingManager {
    code_length: usize,
    code_expiry_secs: u64,
    pending: HashMap<String, PendingPairing>,
    approved: HashMap<String, ApprovedSender>,
}

impl PairingManager {
    pub fn new(code_length: usize, code_expiry_secs: u64) -> Self {
        Self {
            code_length,
            code_expiry_secs,
            pending: HashMap::new(),
            approved: HashMap::new(),
        }
    }

    /// Generate a pairing code for a sender
    pub fn generate_code(&mut self, sender_id: &str) -> Result<String, super::SecurityError> {
        // Clean expired codes
        self.clean_expired();

        // Generate random code
        let code = Self::generate_random_code(self.code_length);

        let pending = PendingPairing {
            sender_id: sender_id.to_string(),
            expires_at: Utc::now() + Duration::seconds(self.code_expiry_secs as i64),
        };

        self.pending.insert(code.clone(), pending);

        Ok(code)
    }

    /// Approve a pairing code
    pub fn approve_code(&mut self, code: &str) -> Result<String, super::SecurityError> {
        // Clean expired codes
        self.clean_expired();

        let pending = self
            .pending
            .remove(code)
            .ok_or_else(|| super::SecurityError::PairingError("Invalid code".to_string()))?;

        if pending.expires_at < Utc::now() {
            return Err(super::SecurityError::PairingError(
                "Code expired".to_string(),
            ));
        }

        let sender_id = pending.sender_id.clone();

        // Add to approved
        self.approved.insert(
            sender_id.clone(),
            ApprovedSender {
                sender_id: sender_id.clone(),
                approved_at: Utc::now(),
            },
        );

        Ok(sender_id)
    }

    /// Check if sender is approved
    pub fn check_sender(&self, sender_id: &str) -> super::PairingStatus {
        if self.approved.contains_key(sender_id) {
            super::PairingStatus::Approved
        } else if self.pending.values().any(|p| p.sender_id == sender_id) {
            super::PairingStatus::Pending
        } else {
            super::PairingStatus::Unknown
        }
    }

    /// Remove an approved sender
    pub fn unpair(&mut self, sender_id: &str) {
        self.approved.remove(sender_id);
    }

    /// List pending codes (for admin)
    pub fn list_pending(&self) -> Vec<(String, chrono::DateTime<Utc>)> {
        self.pending
            .iter()
            .map(|(code, p)| (code.clone(), p.expires_at))
            .collect()
    }

    /// List approved senders
    pub fn list_approved(&self) -> Vec<(String, chrono::DateTime<Utc>)> {
        self.approved
            .values()
            .map(|s| (s.sender_id.clone(), s.approved_at))
            .collect()
    }

    /// Generate random numeric code
    fn generate_random_code(length: usize) -> String {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};

        let s = RandomState::new();
        let mut hasher = s.build_hasher();

        // Use UUID and timestamp for randomness
        hasher.write_u128(Uuid::new_v4().as_u128());
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        );

        let hash = hasher.finish();
        let mut code = String::with_capacity(length);

        for i in 0..length {
            let digit = ((hash >> (i * 4)) % 10) as u8;
            code.push((digit + b'0') as char);
        }

        code
    }

    /// Remove expired pending codes
    fn clean_expired(&mut self) {
        let now = Utc::now();
        self.pending.retain(|_, p| p.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_manager_new() {
        let manager = PairingManager::new(6, 300);
        assert!(manager.pending.is_empty());
        assert!(manager.approved.is_empty());
    }

    #[test]
    fn pairing_manager_generate_code_creates_valid_code() {
        let mut manager = PairingManager::new(6, 300);
        
        let code = manager.generate_code("sender-123").unwrap();
        
        assert_eq!(code.len(), 6);
        // Should be all digits
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn pairing_manager_generate_code_different_codes() {
        let mut manager = PairingManager::new(6, 300);
        
        let code1 = manager.generate_code("sender-1").unwrap();
        let code2 = manager.generate_code("sender-2").unwrap();
        
        assert_ne!(code1, code2);
    }

    #[test]
    fn pairing_manager_check_sender_unknown() {
        let manager = PairingManager::new(6, 300);
        
        let status = manager.check_sender("unknown-sender");
        assert!(matches!(status, super::super::PairingStatus::Unknown));
    }

    #[test]
    fn pairing_manager_check_sender_pending() {
        let mut manager = PairingManager::new(6, 300);
        
        manager.generate_code("sender-pending").unwrap();
        
        let status = manager.check_sender("sender-pending");
        assert!(matches!(status, super::super::PairingStatus::Pending));
    }

    #[test]
    fn pairing_manager_check_sender_approved() {
        let mut manager = PairingManager::new(6, 300);
        
        let code = manager.generate_code("sender-approve").unwrap();
        manager.approve_code(&code).unwrap();
        
        let status = manager.check_sender("sender-approve");
        assert!(matches!(status, super::super::PairingStatus::Approved));
    }

    #[test]
    fn pairing_manager_approve_code_success() {
        let mut manager = PairingManager::new(6, 300);
        
        let code = manager.generate_code("sender-to-approve").unwrap();
        let result = manager.approve_code(&code);
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sender-to-approve");
    }

    #[test]
    fn pairing_manager_approve_invalid_code_fails() {
        let mut manager = PairingManager::new(6, 300);
        
        let result = manager.approve_code("000000");
        
        assert!(result.is_err());
        match result {
            Err(super::super::SecurityError::PairingError(msg)) => {
                assert!(msg.contains("Invalid code"));
            }
            _ => panic!("Expected PairingError for invalid code"),
        }
    }

    #[test]
    fn pairing_manager_approve_moves_to_approved() {
        let mut manager = PairingManager::new(6, 300);
        
        let code = manager.generate_code("sender-move").unwrap();
        
        // Before approval - pending
        assert!(matches!(manager.check_sender("sender-move"), super::super::PairingStatus::Pending));
        
        manager.approve_code(&code).unwrap();
        
        // After approval - approved
        assert!(matches!(manager.check_sender("sender-move"), super::super::PairingStatus::Approved));
        
        // Code should be removed from pending
        assert!(manager.approve_code(&code).is_err());
    }

    #[test]
    fn pairing_manager_unpair_removes_approval() {
        let mut manager = PairingManager::new(6, 300);
        
        let code = manager.generate_code("sender-unpair").unwrap();
        manager.approve_code(&code).unwrap();
        
        assert!(matches!(manager.check_sender("sender-unpair"), super::super::PairingStatus::Approved));
        
        manager.unpair("sender-unpair");
        
        assert!(matches!(manager.check_sender("sender-unpair"), super::super::PairingStatus::Unknown));
    }

    #[test]
    fn pairing_manager_list_pending() {
        let mut manager = PairingManager::new(6, 300);
        
        manager.generate_code("sender-a").unwrap();
        manager.generate_code("sender-b").unwrap();
        
        let pending = manager.list_pending();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn pairing_manager_list_approved() {
        let mut manager = PairingManager::new(6, 300);
        
        let code1 = manager.generate_code("sender-1").unwrap();
        let code2 = manager.generate_code("sender-2").unwrap();
        
        manager.approve_code(&code1).unwrap();
        manager.approve_code(&code2).unwrap();
        
        let approved = manager.list_approved();
        assert_eq!(approved.len(), 2);
        
        let sender_ids: Vec<_> = approved.iter().map(|(id, _)| id.clone()).collect();
        assert!(sender_ids.contains(&"sender-1".to_string()));
        assert!(sender_ids.contains(&"sender-2".to_string()));
    }

    #[test]
    fn pairing_manager_approve_expired_code_fails() {
        let mut manager = PairingManager::new(6, 0); // 0 second expiry
        
        let code = manager.generate_code("sender-expired").unwrap();
        
        // Wait a bit to ensure expiry
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        let result = manager.approve_code(&code);
        assert!(result.is_err());
        match result {
            Err(super::super::SecurityError::PairingError(msg)) => {
                assert!(msg.contains("expired") || msg.contains("Invalid code"));
            }
            _ => {}
        }
    }

    #[test]
    fn pairing_manager_generate_code_cleans_expired() {
        let mut manager = PairingManager::new(6, 0); // 0 second expiry
        
        manager.generate_code("sender-old").unwrap();
        
        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        // Generate new code - should clean expired
        manager.generate_code("sender-new").unwrap();
        
        // Old sender should be unknown now
        assert!(matches!(manager.check_sender("sender-old"), super::super::PairingStatus::Unknown));
    }

    #[test]
    fn pairing_manager_generate_random_code_length() {
        for length in [4, 6, 8, 10] {
            let code = PairingManager::generate_random_code(length);
            assert_eq!(code.len(), length);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn pairing_manager_different_lengths() {
        let manager_short = PairingManager::new(4, 300);
        let manager_long = PairingManager::new(10, 300);
        
        assert_eq!(manager_short.code_length, 4);
        assert_eq!(manager_long.code_length, 10);
    }

    #[test]
    fn pairing_manager_multiple_codes_same_sender() {
        let mut manager = PairingManager::new(6, 300);
        
        // Generate multiple codes for same sender
        let code1 = manager.generate_code("same-sender").unwrap();
        let code2 = manager.generate_code("same-sender").unwrap();
        
        // Both codes should work
        assert!(manager.approve_code(&code1).is_ok());
        
        // Second code might or might not work depending on implementation
        // After approval, sender is approved anyway
        assert!(matches!(manager.check_sender("same-sender"), super::super::PairingStatus::Approved));
    }
}
