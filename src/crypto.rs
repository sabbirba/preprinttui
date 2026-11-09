use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

const JWT_HEADER: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
const JWT_PAYLOAD: &str = "eyJzdWIiOiJwcmludGVyIiwiaXNzIjoicHJlY29ubmVjdCJ9";

pub fn make_jwt(worker_key: &str) -> String {
    let unsigned = format!("{JWT_HEADER}.{JWT_PAYLOAD}");
    let signature = Hmac::<Sha256>::new_from_slice(worker_key.as_bytes())
        .map(|mac| URL_SAFE_NO_PAD.encode(mac.chain_update(&unsigned).finalize().into_bytes()))
        .unwrap_or_default();
    format!("{unsigned}.{signature}")
}
