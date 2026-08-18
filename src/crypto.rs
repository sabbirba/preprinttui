use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn make_jwt(worker_key: &str) -> String {
    let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
    let payload = "eyJzdWIiOiJwcmludGVyIiwiaXNzIjoicHJlY29ubmVjdCJ9";
    let sig_input = format!("{header}.{payload}");
    let signature = if let Ok(mut mac) = HmacSha256::new_from_slice(worker_key.as_bytes()) {
        mac.update(sig_input.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    } else {
        String::new()
    };
    format!("{header}.{payload}.{signature}")
}
