use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::hmac;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub const KIND_HELLO: &str = "hello";
pub const KIND_HEARTBEAT: &str = "heartbeat";
pub const KIND_CLAIM: &str = "claim";
pub const KIND_RESIGN: &str = "resign";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Announce {
    pub v: u8,
    pub kind: String,
    pub cluster_id: String,
    pub node_id: String,
    pub role: String,
    pub term: u64,
    pub eligible: bool,
    pub ntp_verified: bool,
    pub server_epoch_ms: i64,
    pub sent_at_ms: i64,
    pub seq: u64,
    pub nonce: u64,
    pub time_port: u16,
    pub mac: String,
}

impl Announce {
    pub fn sign(&mut self, secret: &[u8]) -> Result<(), String> {
        let payload = mac_payload(self)?;
        self.mac = sign_bytes(secret, &payload)?;
        Ok(())
    }

    pub fn verify(&self, secret: &[u8]) -> bool {
        let Ok(payload) = mac_payload(self) else {
            return false;
        };
        verify_bytes(secret, &payload, &self.mac)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeRequest {
    pub v: u8,
    pub cluster_id: String,
    pub node_id: String,
    pub t1_ms: i64,
    pub nonce: u64,
    pub mac: String,
}

impl TimeRequest {
    pub fn sign(&mut self, secret: &[u8]) -> Result<(), String> {
        let payload = mac_payload(self)?;
        self.mac = sign_bytes(secret, &payload)?;
        Ok(())
    }

    pub fn verify(&self, secret: &[u8]) -> bool {
        let Ok(payload) = mac_payload(self) else {
            return false;
        };
        verify_bytes(secret, &payload, &self.mac)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeResponse {
    pub v: u8,
    pub cluster_id: String,
    pub server_id: String,
    pub term: u64,
    pub t1_ms: i64,
    pub t2_ms: i64,
    pub t3_ms: i64,
    pub verified: bool,
    pub ref_age_ms: u64,
    pub seq: u64,
    pub nonce: u64,
    pub mac: String,
}

impl TimeResponse {
    pub fn sign(&mut self, secret: &[u8]) -> Result<(), String> {
        let payload = mac_payload(self)?;
        self.mac = sign_bytes(secret, &payload)?;
        Ok(())
    }

    pub fn verify(&self, secret: &[u8]) -> bool {
        let Ok(payload) = mac_payload(self) else {
            return false;
        };
        verify_bytes(secret, &payload, &self.mac)
    }
}

fn mac_payload<T: Clone + Serialize>(value: &T) -> Result<Vec<u8>, String> {
    // Every signed structure has a `mac` field. Serialize a copy with the MAC
    // cleared so the same canonical bytes are signed and verified.
    let mut value =
        serde_json::to_value(value).map_err(|err| format!("编码集群消息失败：{err}"))?;
    if let Some(object) = value.as_object_mut() {
        object.insert("mac".to_string(), serde_json::Value::String(String::new()));
    }
    serde_json::to_vec(&value).map_err(|err| format!("编码集群消息失败：{err}"))
}

fn sign_bytes(secret: &[u8], payload: &[u8]) -> Result<String, String> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    Ok(STANDARD.encode(hmac::sign(&key, payload).as_ref()))
}

fn verify_bytes(secret: &[u8], payload: &[u8], encoded: &str) -> bool {
    let Ok(mac) = STANDARD.decode(encoded) else {
        return false;
    };
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(&key, payload, &mac).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announce() -> Announce {
        Announce {
            v: PROTOCOL_VERSION,
            kind: KIND_HEARTBEAT.to_string(),
            cluster_id: "lab".to_string(),
            node_id: "node-00000001".to_string(),
            role: "server".to_string(),
            term: 1,
            eligible: true,
            ntp_verified: true,
            server_epoch_ms: 1_793_789_248_000,
            sent_at_ms: 1_793_789_248_100,
            seq: 7,
            nonce: 42,
            time_port: 18687,
            mac: String::new(),
        }
    }

    #[test]
    fn announce_signature_roundtrip_and_tamper_detection() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let mut message = announce();
        message.sign(secret).unwrap();
        assert!(message.verify(secret));
        message.term += 1;
        assert!(!message.verify(secret));
    }

    #[test]
    fn time_request_and_response_are_authenticated() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let mut request = TimeRequest {
            v: PROTOCOL_VERSION,
            cluster_id: "lab".to_string(),
            node_id: "node-00000001".to_string(),
            t1_ms: 1_793_789_248_000,
            nonce: 9,
            mac: String::new(),
        };
        request.sign(secret).unwrap();
        assert!(request.verify(secret));
        let mut response = TimeResponse {
            v: PROTOCOL_VERSION,
            cluster_id: "lab".to_string(),
            server_id: "node-00000002".to_string(),
            term: 1,
            t1_ms: request.t1_ms,
            t2_ms: request.t1_ms + 2,
            t3_ms: request.t1_ms + 3,
            verified: true,
            ref_age_ms: 100,
            seq: 10,
            nonce: request.nonce,
            mac: String::new(),
        };
        response.sign(secret).unwrap();
        assert!(response.verify(secret));
        response.t3_ms += 1;
        assert!(!response.verify(secret));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let mut message = announce();
        message.sign(b"0123456789abcdef0123456789abcdef").unwrap();
        assert!(!message.verify(b"fedcba9876543210fedcba9876543210"));
    }
}
