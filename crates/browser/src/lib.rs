use http_body_util::BodyExt;
use hyper::Uri;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn renderer_path() -> PathBuf {
    let name = if cfg!(windows) { "renderer.exe" } else { "renderer" };
    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.clone();
        p.pop();
        p.push(name);
        if p.exists() {
            return p;
        }
        let mut p2 = exe;
        p2.pop();
        p2.pop();
        p2.push(name);
        if p2.exists() {
            return p2;
        }
    }
    PathBuf::from(name)
}

async fn fetch_url(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let uri: Uri = url.parse()?;
    let https = HttpsConnector::new();
    let client: Client<_, http_body_util::Full<bytes::Bytes>> =
        Client::builder(TokioExecutor::new()).build(https);

    let res = client.get(uri).await?;
    let body = res.into_body().collect().await?.to_bytes();
    Ok(body.to_vec())
}

pub fn start(url: Option<String>) {
    let rt = tokio::runtime::Runtime::new().unwrap();

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

    if let Some(url) = url {
        println!("[browser] cargando: {}", url);
        match rt.block_on(fetch_url(&url)) {
            Ok(body) => {
                println!("[browser] recibidos {} bytes", body.len());
                write_message(
                    &mut to_renderer,
                    &BrowserToRenderer::LoadHtml {
                        request_id: 2,
                        body: body.clone(),
                    },
                )
                .unwrap();
                let confirm: RendererToBrowser = read_message(&mut from_renderer).unwrap();
                match confirm {
                    RendererToBrowser::HtmlLoaded { body, .. } => {
                        let preview_len = body.len().min(500);
                        let preview = String::from_utf8_lossy(&body[..preview_len]);
                        println!(
                            "[browser] HTML recibido ({} bytes). Primeros {} bytes:\n{}",
                            body.len(),
                            preview_len,
                            preview
                        );
                    }
                    other => println!("[browser] renderer confirma: {:?}", other),
                }
            }
            Err(e) => {
                eprintln!("[browser] error al cargar: {}", e);
                let mut source = e.source();
                while let Some(s) = source {
                    eprintln!("  causado por: {}", s);
                    source = s.source();
                }
            }
        }
    }

    write_message(&mut to_renderer, &BrowserToRenderer::Shutdown).unwrap();
    let _ = child.wait();
    println!("[browser] renderer terminado");
}
