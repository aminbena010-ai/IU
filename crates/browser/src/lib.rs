use ipc::{BrowserToRenderer, RendererToBrowser, encode, decode};

pub fn start() {
    println!("[browser] IU browser arrancado");
    let msg = BrowserToRenderer::Ping { id: 1 };
    let bytes = encode(&msg);
    println!("[browser] enviado Ping ({} bytes)", bytes.len());

    // Simulación de respuesta del renderer
    let reply = RendererToBrowser::Pong { id: 1 };
    let reply_bytes = encode(&reply);
    let decoded: RendererToBrowser = decode(&reply_bytes);
    println!("[browser] recibido: {:?}", decoded);
}