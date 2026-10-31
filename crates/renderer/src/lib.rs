use ipc::{BrowserToRenderer, RendererToBrowser, encode, decode};

pub fn start() {
    println!("[renderer] IU renderer arrancado");
    let msg = BrowserToRenderer::Ping { id: 1 };
    let bytes = encode(&msg);
    let decoded: BrowserToRenderer = decode(&bytes);
    println!("[renderer] recibido: {:?}", decoded);

    let reply = RendererToBrowser::Pong { id: 1 };
    let reply_bytes = encode(&reply);
    println!("[renderer] enviado Pong ({} bytes)", reply_bytes.len());
}