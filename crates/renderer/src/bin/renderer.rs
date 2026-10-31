use cssparser::{Parser, ParserInput, RuleBodyParser};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser, LayoutBox};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::io::{stdin, stdout};
use tiny_skia::{Color, Pixmap};
use fontdue::{Font, FontSettings};

fn parse_html(bytes: &[u8]) -> RcDom {
    let html = String::from_utf8_lossy(bytes).to_string();
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap()
}

fn count_nodes(node: &Handle) -> usize {
    let mut count = 1;
    for child in node.children.borrow().iter() { count += count_nodes(child); }
    count
}

fn max_depth(node: &Handle, depth: usize) -> usize {
    let mut max = depth;
    for child in node.children.borrow().iter() {
        let d = max_depth(child, depth + 1);
        if d > max { max = d; }
    }
    max
}

fn collect_first_elements(node: &Handle, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit { return; }
    if let NodeData::Element { name, .. } = &node.data {
        out.push(name.local.to_string());
    }
    for child in node.children.borrow().iter() {
        collect_first_elements(child, out, limit);
        if out.len() >= limit { return; }
    }
}

fn collect_style_text(node: &Handle, out: &mut String) {
    if let NodeData::Element { name, .. } = &node.data {
        if name.local.as_ref() == "style" {
            for child in node.children.borrow().iter() {
                if let NodeData::Text { contents } = &child.data {
                    out.push_str(&contents.borrow());
                }
            }
        }
    }
    for child in node.children.borrow().iter() { collect_style_text(child, out); }
}

fn collect_inline_styles(node: &Handle, count: &mut usize, examples: &mut Vec<String>) {
    if let NodeData::Element { name, attrs, .. } = &node.data {
        for attr in attrs.borrow().iter() {
            if attr.name.local.as_ref() == "style" && !attr.value.is_empty() {
                *count += 1;
                if examples.len() < 5 {
                    examples.push(format!("<{} style=\"{}\">", name.local, attr.value));
                }
            }
        }
    }
    for child in node.children.borrow().iter() { collect_inline_styles(child, count, examples); }
}

struct CssRuleParser;
impl<'i> cssparser::DeclarationParser<'i> for CssRuleParser {
    type Declaration = (); type Error = ();
    fn parse_value<'t>(&mut self, _name: cssparser::CowRcStr<'i>, _input: &mut Parser<'i, 't>) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> { Ok(()) }
}
impl<'i> cssparser::QualifiedRuleParser<'i> for CssRuleParser {
    type Prelude = (); type QualifiedRule = (); type Error = ();
    fn parse_prelude<'t>(&mut self, _input: &mut Parser<'i, 't>) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> { Ok(()) }
    fn parse_block<'t>(&mut self, _prelude: Self::Prelude, _state: &cssparser::ParserState, _input: &mut Parser<'i, 't>) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>> { Ok(()) }
}
impl<'i> cssparser::AtRuleParser<'i> for CssRuleParser {
    type Prelude = (); type AtRule = (); type Error = ();
}
impl<'i> cssparser::RuleBodyItemParser<'i, (), ()> for CssRuleParser {
    fn parse_declarations(&self) -> bool { false }
    fn parse_qualified(&self) -> bool { false }
}

fn count_css_rules(css: &str) -> usize {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut rule_parser = CssRuleParser;
    let mut count = 0;
    for _rule in RuleBodyParser::new(&mut parser, &mut rule_parser) { count += 1; }
    count
}

// ===== LAYOUT =====
fn layout_node(node: &Handle, x: f32, y: f32, width: f32, boxes: &mut Vec<LayoutBox>, depth: usize) -> f32 {
    if depth > 50 { return 0.0; }
    let padding = 4.0;
    let line_height = 18.0;
    let mut total_height = 0.0f32;

    match &node.data {
        NodeData::Element { name, .. } => {
            let tag = name.local.to_string();
            // Ignorar etiquetas que no se ven en pantalla
            if matches!(tag.as_str(), "meta" | "title" | "script" | "style" | "head" | "link") {
                return 0.0;
            }

            let inner_x = x + padding;
            let inner_y = y + padding;
            let inner_width = (width - padding * 2.0).max(10.0);
            let mut children_height = 0.0f32;

            for child in node.children.borrow().iter() {
                let h = layout_node(child, inner_x, inner_y + children_height, inner_width, boxes, depth + 1);
                children_height += h;
            }

            let height = if children_height > 0.0 { children_height + padding * 2.0 } else { line_height };
            boxes.push(LayoutBox { tag, x, y, width, height, text: "".to_string() });
            total_height = height;
        }
        NodeData::Text { contents } => {
            let text = contents.borrow().trim().to_string();
            if !text.is_empty() {
                boxes.push(LayoutBox {
                    tag: "#text".to_string(), x, y, width: (text.len() as f32 * 8.0).min(width), height: line_height, text,
                });
                total_height = line_height;
            }
        }
        _ => {
            let mut child_y = y;
            for child in node.children.borrow().iter() {
                let h = layout_node(child, x, child_y, width, boxes, depth + 1);
                child_y += h;
                total_height += h;
            }
        }
    }
    total_height
}

fn compute_layout(dom: &RcDom) -> (usize, f32, f32, Vec<LayoutBox>) {
    let viewport_width = 1024.0f32;
    let mut boxes = Vec::new();
    let height = layout_node(&dom.document, 0.0, 0.0, viewport_width, &mut boxes, 0);
    let first_boxes: Vec<LayoutBox> = boxes.iter().take(10).cloned().collect();
    (boxes.len(), viewport_width, height, boxes) // Devolvemos TODAS las cajas
}

// ===== PAINT =====
fn paint_boxes(boxes: &[LayoutBox], width: u32, height: u32) -> Vec<u8> {
    let mut pixmap = Pixmap::new(width, height).unwrap();
    pixmap.fill(Color::WHITE);

    let font_data = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").unwrap_or_default();
    let font = Font::from_bytes(font_data, FontSettings::default()).ok();

    for b in boxes {
        if b.tag == "#text" && !b.text.is_empty() {
            if let Some(ref f) = font {
                let mut x_offset = b.x;
                let y_offset = b.y + 14.0; // Línea base
                for c in b.text.chars().take(100) {
                    let (metrics, bitmap) = f.rasterize(c, 14.0);
                    for y_px in 0..metrics.height {
                        for x_px in 0..metrics.width {
                            let val = bitmap[y_px * metrics.width + x_px];
                            if val > 0 {
                                let px = (x_offset + metrics.xmin as f32 + x_px as f32) as u32;
                                let py = (y_offset - metrics.ymin as f32 - y_px as f32) as u32;
                                if px < width && py < height {
                                    let idx = ((py * width + px) * 4) as usize;
                                    let data = pixmap.data_mut();
                                    data[idx] = 0; data[idx+1] = 0; data[idx+2] = 0; data[idx+3] = 255;
                                }
                            }
                        }
                    }
                    x_offset += metrics.advance_width;
                }
            }
        }
    }
    pixmap.data().to_vec()
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
                let _ = write_message(&mut output, &RendererToBrowser::DomParsed { request_id, node_count, max_depth, first_elements });

                let mut css_text = String::new();
                collect_style_text(&dom.document, &mut css_text);
                let rule_count = count_css_rules(&css_text);
                let mut styled_count = 0usize;
                let mut examples = Vec::new();
                collect_inline_styles(&dom.document, &mut styled_count, &mut examples);
                let _ = write_message(&mut output, &RendererToBrowser::StylesComputed { request_id, rule_count, styled_element_count: styled_count, examples });

                let (box_count, root_width, root_height, all_boxes) = compute_layout(&dom);
                let first_boxes = all_boxes.iter().take(10).cloned().collect();
                let _ = write_message(&mut output, &RendererToBrowser::LayoutComputed { request_id, box_count, root_width, root_height, first_boxes });

                let paint_width = root_width as u32;
                let paint_height = (root_height as u32).min(3000);
                let pixels = paint_boxes(&all_boxes, paint_width, paint_height);
                let _ = write_message(&mut output, &RendererToBrowser::PaintRendered { request_id, width: paint_width, height: paint_height, pixels });

                // H9 JS (simplificado)
                let _ = write_message(&mut output, &RendererToBrowser::JsExecuted { script_count: 0, success_count: 0 });
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}
