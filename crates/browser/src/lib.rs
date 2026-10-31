use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn renderer_path() -> PathBuf {
    let name = if cfg!(windows) { "renderer.exe" } else { "renderer" };

    // 1. Intentar junto al ejecutable actual (target/debug o target/release)
    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.clone();
        p.pop();
        p.push(name);
        if p.exists() {
            return p;
        }
        // 2. Intentar subir un nivel (por si el demo está en target/debug/deps o similar)
        let mut p2 = exe;
        p2.pop();
        p2.pop();
        p2.push(name);
        if p2.exists() {
            return p2;
        }
    }

    // 3. Fallback: confiar en el PATH
    PathBuf::from(name)
}

pub fn start() {
    let path = renderer_path();
    println!("[browser] arrancando renderer en: {}", path.display());

    let mut child = Command::new(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("no se pudo lanzar {}: {}", path.display(), e));

    let mut to_renderer = child.stdin.take().unwrap();
    let mut from_renderer = child.stdout.take().unwrap();

    write_message(&mut to_renderer, &BrowserToRenderer::Ping { id: 1 }).unwrap();
    let reply: RendererToBrowser = read_message(&mut from_renderer).unwrap();
    println!("[browser] recibido: {:?}", reply);

    write_message(&mut to_renderer, &BrowserToRenderer::Shutdown).unwrap();
    let _ = child.wait();
    println!("[browser] renderer terminado");
}
