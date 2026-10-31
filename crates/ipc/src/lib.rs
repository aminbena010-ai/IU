use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum BrowserToRenderer {
    Ping { id: u64 },
    LoadUrl { url: String, request_id: u64 },
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RendererToBrowser {
    Pong { id: u64 },
    RequestResource { request_id: u64, url: String },
    Crashed { reason: String },
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    bincode::serialize(msg).expect("encode falló")
}

pub fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> T {
    bincode::deserialize(bytes).expect("decode falló")
}