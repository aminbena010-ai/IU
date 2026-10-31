use cssparser::{Parser, ParserInput, RuleBodyParser};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use ipc::{read_message, write_message, BrowserToRenderer, RendererToBrowser};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::io::{stdin, stdout};

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
            }
            BrowserToRenderer::Shutdown => break,
            BrowserToRenderer::LoadUrl { .. } => {}
        }
    }
}
