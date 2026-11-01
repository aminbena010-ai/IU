use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Serialize, Deserialize, Debug)]
pub enum BrowserToRenderer {
    Ping { id: u64 },
    LoadUrl { url: String, request_id: u64 },
    LoadHtml { request_id: u64, body: Vec<u8> },
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayoutBox {
    pub tag: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub bg_color: Option<(u8, u8, u8)>,
    pub text_color: Option<(u8, u8, u8)>,
    pub font_size: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RendererToBrowser {
    Pong { id: u64 },
    RequestResource { request_id: u64, url: String },
    HtmlLoaded { request_id: u64, body: Vec<u8> },
    DomParsed {
        request_id: u64,
        node_count: usize,
        max_depth: usize,
        first_elements: Vec<String>,
    },
    StylesComputed {
        request_id: u64,
        rule_count: usize,
        styled_element_count: usize,
        examples: Vec<String>,
    },
    LayoutComputed {
        request_id: u64,
        box_count: usize,
        root_width: f32,
        root_height: f32,
        first_boxes: Vec<LayoutBox>,
    },
    PaintRendered {
        request_id: u64,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
    JsExecuted { script_count: usize, success_count: usize },
    Crashed { reason: String },
}

pub fn write_message<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let payload = bincode::serialize(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let len = (payload.len() as u32).to_le_bytes();
    w.write_all(&len)?;
    w.write_all(&payload)?;
    w.flush()
}

pub fn read_message<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    bincode::deserialize(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}
