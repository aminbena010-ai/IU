use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use std::io::{stdin, stdout};

fn main() {
    let mut input = stdin().lock();
    let mut output = stdout().lock();

    loop {
        let msg: BrowserToRenderer = match read_message(&mut input) {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            BrowserToRenderer::Ping { id } => {
                let _ = write_message(&mut output, &RendererToBrowser::Pong { id });
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}
