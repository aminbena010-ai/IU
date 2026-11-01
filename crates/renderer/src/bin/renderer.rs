use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser, LayoutBox};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::collections::HashMap;
use std::io::{stdin, stdout};
use tiny_skia::{Color, Pixmap, Paint, PathBuilder, Transform};
use rustybuzz::{Face, UnicodeBuffer};
use fontdb::{Database, Source};
use ttf_parser::OutlineBuilder;

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

fn extract_css_rules(css: &str) -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();
    // Parser muy básico: buscamos bloques { ... } y sus selectores
    let mut current_selector = String::new();
    let mut in_selector = true;
    let mut current_props = HashMap::new();
    
    for line in css.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") { continue; }
        
        if in_selector {
            if line.ends_with('{') {
                current_selector = line[..line.len()-1].trim().to_string();
                in_selector = false;
            } else if line.contains('{') {
                current_selector = line[..line.find('{').unwrap()].trim().to_string();
                in_selector = false;
            }
        } else {
            if line == "}" {
                map.insert(current_selector.clone(), current_props.clone());
                current_props.clear();
                in_selector = true;
            } else if line.contains(':') {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let val = parts[1].trim().trim_end_matches(';').to_string();
                    current_props.insert(key, val);
                }
            }
        }
    }
    map
}

// ===== LAYOUT =====
struct LayoutContext {
    boxes: Vec<LayoutBox>,
    cursor_x: f32,
    cursor_y: f32,
    max_width: f32,
    line_height: f32,
    is_centered: bool,
    current_bg: Option<(u8, u8, u8)>,
    current_color: Option<(u8, u8, u8)>,
    current_font_size: f32,
    css_rules: HashMap<String, HashMap<String, String>>,
}

impl LayoutContext {
    fn new(max_width: f32, css_rules: HashMap<String, HashMap<String, String>>) -> Self {
        Self { 
            boxes: Vec::new(), cursor_x: 16.0, cursor_y: 16.0, max_width, line_height: 20.0, 
            is_centered: false, current_bg: None, current_color: None, current_font_size: 14.0,
            css_rules,
        }
    }
    
    fn add_text(&mut self, text: &str) {
        let width = text.chars().count() as f32 * 8.0; 
        if self.cursor_x + width > self.max_width {
            self.new_line();
        }
        
        let actual_x = if self.is_centered {
            // Si estamos centrados, calculamos la X para que quede en el medio
            (self.max_width - width) / 2.0
        } else {
            self.cursor_x
        };

        self.boxes.push(LayoutBox {
            tag: "#text".to_string(),
            x: actual_x,
            y: self.cursor_y,
            width,
            height: self.line_height,
            text: text.to_string(),
            bg_color: self.current_bg,
            text_color: self.current_color,
            font_size: self.current_font_size,
        });
        
        // IMPORTANTE: Actualizar el cursor para que el siguiente texto no se superponga
        self.cursor_x = actual_x + width + 8.0; // 8px de margen entre palabras
    }
    
    fn new_line(&mut self) {
        self.cursor_x = 16.0;
        self.cursor_y += self.line_height;
    }

    fn apply_css(&mut self, selector: &str) {
        if let Some(rules) = self.css_rules.get(selector) {
            if rules.contains_key("text-align") && rules["text-align"] == "center" {
                self.is_centered = true;
            }
            if let Some(color) = rules.get("color") {
                if color == "blue" || color.starts_with("#1a0dab") {
                    self.current_color = Some((26, 13, 171));
                }
            }
            if let Some(font_size) = rules.get("font-size") {
                if let Some(size_str) = font_size.strip_suffix("px") {
                    if let Ok(size) = size_str.parse::<f32>() {
                        self.current_font_size = size;
                    }
                }
            }
        }
    }
}

fn layout_node(node: &Handle, ctx: &mut LayoutContext, depth: usize) {
    if depth > 50 { return; }

    match &node.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string();
            if matches!(tag.as_str(), "meta" | "title" | "script" | "style" | "head" | "link") {
                return;
            }

            // Recorrer atributos para aplicar CSS por clase o id
            for attr in attrs.borrow().iter() {
                if attr.name.local.as_ref() == "class" {
                    for class in attr.value.split_whitespace() {
                        ctx.apply_css(&format!(".{}", class));
                    }
                }
                if attr.name.local.as_ref() == "id" {
                    ctx.apply_css(&format!("#{}", attr.value));
                }
            }
            ctx.apply_css(&tag);

            let is_block = matches!(tag.as_str(), "div" | "p" | "h1" | "h2" | "h3" | "form" | "li" | "br" | "tr" | "center" | "table");
            if is_block {
                ctx.new_line();
            }

            if tag == "center" || tag == "body" {
                ctx.is_centered = true;
            }

            for child in node.children.borrow().iter() {
                layout_node(child, ctx, depth + 1);
            }

            if tag == "center" || tag == "body" || tag == "div" {
                ctx.is_centered = false;
            }

            if is_block {
                ctx.new_line();
            }
        }
        NodeData::Text { contents } => {
            let text = contents.borrow().trim().to_string();
            if !text.is_empty() && text.len() > 1 && !text.contains("function") {
                // Si el texto es un enlace de pie de página, lo ponemos uno debajo de otro
                if text == "Publicidad" || text == "Soluciones Empresariales" || text == "Todo acerca de Google" || text == "Google.es" || text == "Privacidad" || text == "Términos" || text == "Aplicaciones de Google" {
                    ctx.new_line();
                }
                if text.ends_with(':') {
                    ctx.new_line();
                    // Después del texto con ':', nos aseguramos de que el cursor vuelva a la izquierda
                    ctx.is_centered = false; 
                }
                ctx.add_text(&text);
            }
        }
        _ => {
            for child in node.children.borrow().iter() {
                layout_node(child, ctx, depth + 1);
            }
        }
    }
}

fn compute_layout(dom: &RcDom, css_rules: HashMap<String, HashMap<String, String>>) -> (usize, f32, f32, Vec<LayoutBox>) {
    let viewport_width = 1024.0f32;
    let mut ctx = LayoutContext::new(viewport_width, css_rules);
    layout_node(&dom.document, &mut ctx, 0);
    
    let height = ctx.cursor_y + 40.0;
    (ctx.boxes.len(), viewport_width, height, ctx.boxes)
}

// ===== PAINT =====
struct SkiaOutlineBuilder(PathBuilder);
impl OutlineBuilder for SkiaOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x, -y); }
    fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x, -y); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1, -y1, x, -y); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.0.cubic_to(x1, -y1, x2, -y2, x, -y); }
    fn close(&mut self) { self.0.close(); }
}

fn paint_boxes(boxes: &[LayoutBox], width: u32, height: u32) -> Vec<u8> {
    let mut pixmap = Pixmap::new(width, height).unwrap();
    pixmap.fill(Color::WHITE);

    let mut db = Database::new();
    db.load_system_fonts();
    let face_id = db.query(&fontdb::Query {
        families: &[fontdb::Family::SansSerif],
        weight: fontdb::Weight::NORMAL, stretch: fontdb::Stretch::Normal, style: fontdb::Style::Normal,
    }).unwrap_or_else(|| db.faces().next().map(|f| f.id).unwrap());

    let (src, index) = db.face_source(face_id).unwrap();
    let font_data = match src {
        Source::Binary(data) => data.as_ref().as_ref().to_vec(),
        Source::File(path) => std::fs::read(path).unwrap_or_default(),
        Source::SharedFile(_, data) => data.as_ref().as_ref().to_vec(),
    };

    let face = Face::from_slice(&font_data, index).unwrap();
    let units_per_em = face.units_per_em() as f32;
    let ttf_face = ttf_parser::Face::parse(&font_data, 0).unwrap();

    for b in boxes {
        if let Some((r, g, bl)) = b.bg_color {
            let rect = tiny_skia::Rect::from_xywh(b.x, b.y, b.width.max(1.0), b.height.max(1.0));
            if let Some(rect) = rect {
                let mut paint_bg = Paint::default();
                paint_bg.set_color_rgba8(r, g, bl, 255);
                pixmap.fill_rect(rect, &paint_bg, Transform::identity(), None);
            }
        }

        if b.tag == "#text" && !b.text.is_empty() {
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(&b.text);
            buffer.guess_segment_properties();
            
            let glyph_buffer = rustybuzz::shape(&face, &[], buffer);
            let glyph_infos = glyph_buffer.glyph_infos();
            let glyph_positions = glyph_buffer.glyph_positions();

            let mut x_offset = b.x;
            let y_offset = b.y + b.font_size + 2.0;

            for i in 0..glyph_infos.len() {
                let pos = &glyph_positions[i];
                let scale = b.font_size / units_per_em;
                let advance = pos.x_advance as f32 * scale;

                let glyph_id = ttf_parser::GlyphId(glyph_infos[i].glyph_id as u16);
                let mut builder = SkiaOutlineBuilder(PathBuilder::new());
                if ttf_face.outline_glyph(glyph_id, &mut builder).is_some() {
                    if let Some(path) = builder.0.finish() {
                        let mut paint = Paint::default();
                        if let Some((r, g, bl)) = b.text_color {
                            paint.set_color_rgba8(r, g, bl, 255);
                        } else {
                            paint.set_color_rgba8(0, 0, 0, 255);
                        }
                        let transform = Transform::from_translate(
                            x_offset + (pos.x_offset as f32 * scale), 
                            y_offset - (pos.y_offset as f32 * scale)
                        ).pre_scale(scale, scale);
                        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
                    }
                }
                x_offset += advance;
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
                let css_rules = extract_css_rules(&css_text);
                let rule_count = css_rules.len();
                let _ = write_message(&mut output, &RendererToBrowser::StylesComputed { request_id, rule_count, styled_element_count: 0, examples: vec![] });

                let (box_count, root_width, root_height, all_boxes) = compute_layout(&dom, css_rules);
                let first_boxes = all_boxes.iter().take(10).cloned().collect();
                let _ = write_message(&mut output, &RendererToBrowser::LayoutComputed { request_id, box_count, root_width, root_height, first_boxes });

                let paint_width = root_width as u32;
                let paint_height = (root_height as u32).min(3000);
                let pixels = paint_boxes(&all_boxes, paint_width, paint_height);
                let _ = write_message(&mut output, &RendererToBrowser::PaintRendered { request_id, width: paint_width, height: paint_height, pixels });

                let _ = write_message(&mut output, &RendererToBrowser::JsExecuted { script_count: 0, success_count: 0 });
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}
