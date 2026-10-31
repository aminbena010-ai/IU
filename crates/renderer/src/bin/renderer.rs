use cssparser::{Parser, ParserInput, RuleBodyParser};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::io::{stdin, stdout};
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

fn parse_html(bytes: &[u8]) -> RcDom {
    let html = String::from_utf8_lossy(bytes).to_string();
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap()
}

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
    for child in node.children.borrow().iter() {
        collect_style_text(child, out);
    }
}

fn collect_inline_styles(node: &Handle, count: &mut usize, examples: &mut Vec<String>) {
    if let NodeData::Element { name, attrs, .. } = &node.data {
        for attr in attrs.borrow().iter() {
            if attr.name.local.as_ref() == "style" && !attr.value.is_empty() {
                *count += 1;
                if examples.len() < 5 {
                    examples.push(format!(
                        "<{} style=\"{}\">",
                        name.local, attr.value
                    ));
                }
            }
        }
    }
    for child in node.children.borrow().iter() {
        collect_inline_styles(child, count, examples);
    }
}

/// Parser mínimo que consume cualquier bloque CSS y cuenta reglas.
struct CssRuleParser;

impl<'i> cssparser::DeclarationParser<'i> for CssRuleParser {
    type Declaration = ();
    type Error = ();
    fn parse_value<'t>(
        &mut self,
        _name: cssparser::CowRcStr<'i>,
        _input: &mut Parser<'i, 't>,
    ) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> {
        Ok(())
    }
}

impl<'i> cssparser::QualifiedRuleParser<'i> for CssRuleParser {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = ();
    fn parse_prelude<'t>(
        &mut self,
        _input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        Ok(())
    }
    fn parse_block<'t>(
        &mut self,
        _prelude: Self::Prelude,
        _state: &cssparser::ParserState,
        _input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>> {
        Ok(())
    }
}

impl<'i> cssparser::AtRuleParser<'i> for CssRuleParser {
    type Prelude = ();
    type AtRule = ();
    type Error = ();
}

impl<'i> cssparser::RuleBodyItemParser<'i, (), ()> for CssRuleParser {
    fn parse_declarations(&self) -> bool {
        false
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

fn count_css_rules(css: &str) -> usize {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut rule_parser = CssRuleParser;
    let mut count = 0;
    for _rule in RuleBodyParser::new(&mut parser, &mut rule_parser) {
        count += 1;
    }
    count
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

                let mut css_text = String::new();
                collect_style_text(&dom.document, &mut css_text);
                let rule_count = count_css_rules(&css_text);

                let mut styled_count = 0usize;
                let mut examples = Vec::new();
                collect_inline_styles(&dom.document, &mut styled_count, &mut examples);

                if rule_count > 0 && examples.len() < 5 {
                    examples.push(format!("<style> con {} reglas CSS", rule_count));
                }

                let _ = write_message(
                    &mut output,
                    &RendererToBrowser::StylesComputed {
                        request_id,
                        rule_count,
                        styled_element_count: styled_count,
                        examples,
                    },
                );

                // H5: layout
                let (box_count, root_width, root_height, first_boxes) = compute_layout(&dom);
                let _ = write_message(
                    &mut output,
                    &RendererToBrowser::LayoutComputed {
                        request_id,
                        box_count,
                        root_width,
                        root_height,
                        first_boxes: first_boxes.clone(),
                    },
                );

                // H6: pintar
                let paint_width = root_width as u32;
                let paint_height = (root_height as u32).min(2000);
                let pixels = paint_boxes(&first_boxes, paint_width, paint_height);
                let _ = write_message(
                    &mut output,
                    &RendererToBrowser::PaintRendered {
                        request_id,
                        width: paint_width,
                        height: paint_height,
                        pixels,
                    },
                );
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}

// ===== H5: Layout básico =====
// Recorre el DOM y genera una caja por cada elemento visible.
fn layout_node(
    node: &Handle,
    x: f32,
    y: f32,
    width: f32,
    boxes: &mut Vec<ipc::LayoutBox>,
    depth: usize,
) -> f32 {
    if depth > 50 {
        return 0.0;
    }

    let padding = 4.0;
    let line_height = 16.0;
    let mut total_height = 0.0f32;

    match &node.data {
        NodeData::Element { name, .. } => {
            let tag = name.local.to_string();

            // Calcular altura de los hijos primero
            let inner_x = x + padding;
            let inner_y = y + padding;
            let inner_width = (width - padding * 2.0).max(0.0);
            let mut children_height = 0.0f32;

            for child in node.children.borrow().iter() {
                let h = layout_node(child, inner_x, inner_y + children_height, inner_width, boxes, depth + 1);
                children_height += h;
            }

            let height = if children_height > 0.0 {
                children_height + padding * 2.0
            } else {
                line_height
            };

            boxes.push(ipc::LayoutBox {
                tag,
                x,
                y,
                width,
                height,
            });

            total_height = height;
        }
        NodeData::Text { contents } => {
            if !contents.borrow().trim().is_empty() {
                total_height = line_height;
            }
        }
        _ => {
            // Document, Comment, Doctype: recorrer hijos sin crear caja
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

fn paint_boxes(boxes: &[ipc::LayoutBox], width: u32, height: u32) -> Vec<u8> {
    let mut pixmap = Pixmap::new(width, height).unwrap();
    pixmap.fill(Color::WHITE);

    let mut paint = Paint::default();
    paint.set_color_rgba8(80, 140, 220, 255); // azul
    paint.anti_alias = false;

    for b in boxes {
        let rect = Rect::from_xywh(b.x, b.y, b.width.max(1.0), b.height.max(1.0));
        if let Some(rect) = rect {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    pixmap.data().to_vec()
}

fn compute_layout(dom: &RcDom) -> (usize, f32, f32, Vec<ipc::LayoutBox>) {
    let viewport_width = 1024.0f32;
    let mut boxes = Vec::new();
    let height = layout_node(&dom.document, 0.0, 0.0, viewport_width, &mut boxes, 0);
    let first_boxes: Vec<ipc::LayoutBox> = boxes.iter().take(10).cloned().collect();
    (boxes.len(), viewport_width, height, first_boxes)
}
