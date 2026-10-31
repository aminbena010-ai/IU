use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::io::{stdin, stdout};

fn count_nodes(node: &Handle) -> usize {
    let mut count = 1;
    for child in node.children.borrow().iter() {
        count += count_nodes(child);
    }
    count
}

fn max_depth(node: &Handle, depth: usize) -> usize {
    let mut max = depth;
    for child in node.children.borrow().iter() {
        let d = max_depth(child, depth + 1);
        if d > max {
            max = d;
        }
    }
    max
}

fn collect_first_elements(node: &Handle, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    if let NodeData::Element { name, .. } = &node.data {
        out.push(name.local.to_string());
    }
    for child in node.children.borrow().iter() {
        collect_first_elements(child, out, limit);
        if out.len() >= limit {
            return;
        }
    }
}

fn parse_html(bytes: &[u8]) -> RcDom {
    let html = String::from_utf8_lossy(bytes).to_string();
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap()
}

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
            BrowserToRenderer::LoadHtml { request_id, body } => {
                let dom = parse_html(&body);
                let node_count = count_nodes(&dom.document);
                let max_depth = max_depth(&dom.document, 0);
                let mut first_elements = Vec::new();
                collect_first_elements(&dom.document, &mut first_elements, 10);

                let _ = write_message(
                    &mut output,
                    &RendererToBrowser::DomParsed {
                        request_id,
                        node_count,
                        max_depth,
                        first_elements,
                    },
                );
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}
