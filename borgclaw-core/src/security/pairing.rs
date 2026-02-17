//! Pairing module - sender authentication with pairing codes

use chrono::{Duration, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// Pending pairing request
#[derive(Debug, Clone)]
struct PendingPairing {
    sender_id: String,
    code: String,
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
            code: code.clone(),
            expires_at: Utc::now() + Duration::seconds(self.code_expiry_secs as i64),
        };
        
        self.pending.insert(code.clone(), pending);
        
        Ok(code)
    }
    
    /// Approve a pairing code
    pub fn approve_code(&mut self, code: &str) -> Result<String, super::SecurityError> {
        // Clean expired codes
        self.clean_expired();
        
        let pending = self.pending
            .remove(code)
            .ok_or_else(|| super::SecurityError::PairingError("Invalid code".to_string()))?;
        
        if pending.expires_at < Utc::now() {
            return Err(super::SecurityError::PairingError("Code expired".to_string()));
        }
        
        let sender_id = pending.sender_id.clone();
        
        // Add to approved
        self.approved.insert(sender_id.clone(), ApprovedSender {
            sender_id: sender_id.clone(),
            approved_at: Utc::now(),
        });
        
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
        hasher.write_u128(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos());
        
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
