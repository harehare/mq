use mq_markdown::Node;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TreeItem {
    pub node: Node,
    pub display_text: String,
    pub depth: usize,
    pub is_expanded: bool,
    pub has_children: bool,
    pub index: usize,
}

impl TreeItem {
    pub fn new(node: Node, depth: usize, index: usize) -> Self {
        let display_text = Self::create_display_text(&node);
        let has_children = Self::has_children(&node);

        Self {
            node,
            display_text,
            depth,
            is_expanded: false,
            has_children,
            index,
        }
    }

    fn create_display_text(node: &Node) -> String {
        match node {
            Node::Heading(h) => {
                let text = h
                    .values
                    .iter()
                    .map(|n| n.value().trim().to_string())
                    .collect::<String>();
                format!("H{} {}", h.depth, text)
            }
            Node::List(l) => {
                let item_count = l.values.len();
                if l.ordered {
                    format!("Ordered List ({} items)", item_count)
                } else {
                    format!("Unordered List ({} items)", item_count)
                }
            }
            Node::Code(c) => {
                let lang = c.lang.as_deref().unwrap_or("text");
                format!("Code Block ({})", lang)
            }
            Node::Blockquote(_) => "Blockquote".to_string(),
            Node::Strong(_) => "Strong".to_string(),
            Node::Emphasis(_) => "Emphasis".to_string(),
            Node::Link(link) => {
                let text = link
                    .values
                    .iter()
                    .map(|n| n.value().trim().to_string())
                    .collect::<String>();
                format!("Link: {}", text)
            }
            Node::Image(img) => {
                format!("Image: {}", img.alt)
            }
            Node::Text(t) => {
                let text = t.value.trim();
                if text.len() > 50 {
                    format!("Text: {}...", &text[..47])
                } else {
                    format!("Text: {}", text)
                }
            }
            Node::HorizontalRule(_) => "Horizontal Rule".to_string(),
            Node::TableHeader(_) => "Table Header".to_string(),
            Node::TableRow(_) => "Table Row".to_string(),
            Node::TableCell(_) => "Table Cell".to_string(),
            Node::Break(_) => "Line Break".to_string(),
            Node::Html(h) => format!("HTML: {}", h.value.trim()),
            Node::Math(m) => format!("Math: {}", m.value.trim()),
            Node::MathInline(m) => format!("Inline Math: {}", m.value.trim()),
            Node::CodeInline(c) => format!("Inline Code: {}", c.value.trim()),
            Node::Delete(_) => "Strikethrough".to_string(),
            Node::Yaml(y) => format!("YAML: {}", y.value.trim()),
            Node::Toml(t) => format!("TOML: {}", t.value.trim()),
            Node::Fragment(_) => "Fragment".to_string(),
            Node::Footnote(_) => "Footnote".to_string(),
            Node::FootnoteRef(r) => format!("Footnote Ref: {}", r.ident),
            Node::Definition(d) => format!("Definition: {}", d.ident),
            Node::ImageRef(r) => format!("Image Ref: {}", r.ident),
            Node::LinkRef(r) => format!("Link Ref: {}", r.ident),
            Node::MdxFlowExpression(_) => "MDX Flow Expression".to_string(),
            Node::MdxJsxFlowElement(e) => {
                let name = e.name.as_deref().unwrap_or("element");
                format!("MDX JSX Element: {}", name)
            }
            Node::MdxJsxTextElement(e) => {
                let name = e.name.as_deref().unwrap_or("element");
                format!("MDX JSX Text: {}", name)
            }
            Node::MdxTextExpression(_) => "MDX Text Expression".to_string(),
            Node::MdxJsEsm(_) => "MDX JS ESM".to_string(),
            Node::Empty => "Empty".to_string(),
        }
    }

    fn has_children(node: &Node) -> bool {
        match node {
            Node::Heading(h) => !h.values.is_empty(),
            Node::List(l) => !l.values.is_empty(),
            Node::Blockquote(b) => !b.values.is_empty(),
            Node::Strong(s) => !s.values.is_empty(),
            Node::Emphasis(e) => !e.values.is_empty(),
            Node::Link(l) => !l.values.is_empty(),
            Node::Delete(d) => !d.values.is_empty(),
            Node::Fragment(f) => !f.values.is_empty(),
            Node::Footnote(f) => !f.values.is_empty(),
            Node::TableRow(r) => !r.values.is_empty(),
            Node::TableCell(c) => !c.values.is_empty(),
            Node::MdxJsxFlowElement(e) => !e.children.is_empty(),
            Node::MdxJsxTextElement(e) => !e.children.is_empty(),
            _ => false,
        }
    }

    pub fn get_children(&self) -> Vec<Node> {
        match &self.node {
            Node::Heading(h) => h.values.clone(),
            Node::List(l) => l.values.clone(),
            Node::Blockquote(b) => b.values.clone(),
            Node::Strong(s) => s.values.clone(),
            Node::Emphasis(e) => e.values.clone(),
            Node::Link(l) => l.values.clone(),
            Node::Delete(d) => d.values.clone(),
            Node::Fragment(f) => f.values.clone(),
            Node::Footnote(f) => f.values.clone(),
            Node::TableRow(r) => r.values.clone(),
            Node::TableCell(c) => c.values.clone(),
            Node::MdxJsxFlowElement(e) => e.children.clone(),
            Node::MdxJsxTextElement(e) => e.children.clone(),
            _ => vec![],
        }
    }
}

pub struct TreeView {
    items: Vec<TreeItem>,
    selected_index: usize,
    expanded_items: HashMap<usize, bool>,
    original_nodes: Vec<Node>,
}

impl TreeView {
    pub fn new(nodes: Vec<Node>) -> Self {
        let mut tree = Self {
            items: Vec::new(),
            selected_index: 0,
            expanded_items: HashMap::new(),
            original_nodes: nodes.clone(),
        };

        tree.rebuild_items();
        tree
    }

    pub fn rebuild_items(&mut self) {
        self.items.clear();
        let mut index = 0;
        let nodes = self.original_nodes.clone();

        for node in nodes {
            self.add_node_recursive(node, 0, &mut index);
        }
    }

    fn add_node_recursive(&mut self, node: Node, depth: usize, index: &mut usize) {
        let current_index = *index;
        let tree_item = TreeItem::new(node, depth, current_index);
        let is_expanded = *self
            .expanded_items
            .get(&current_index)
            .unwrap_or(&tree_item.is_expanded);

        let mut item = tree_item;
        item.is_expanded = is_expanded;

        let children = item.get_children();
        self.items.push(item);
        *index += 1;

        if is_expanded && !children.is_empty() {
            for child in children {
                self.add_node_recursive(child, depth + 1, index);
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
        }
    }

    pub fn toggle_expand(&mut self) {
        if let Some(item) = self.items.get(self.selected_index) {
            if item.has_children {
                let current_expanded = item.is_expanded;
                self.expanded_items.insert(item.index, !current_expanded);
                self.rebuild_items();

                self.selected_index = self.selected_index.min(self.items.len().saturating_sub(1));
            }
        }
    }

    pub fn get_selected_node(&self) -> Option<&Node> {
        self.items.get(self.selected_index).map(|item| &item.node)
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn items(&self) -> &[TreeItem] {
        &self.items
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, tree_item)| {
                let indent = "  ".repeat(tree_item.depth);
                let expand_icon = if tree_item.has_children {
                    if tree_item.is_expanded {
                        "▼ "
                    } else {
                        "▶ "
                    }
                } else {
                    "  "
                };

                let content = format!("{}{}{}", indent, expand_icon, tree_item.display_text);
                let line = Line::from(vec![Span::styled(
                    content,
                    if i == self.selected_index {
                        Style::default().fg(Color::Black).bg(Color::White)
                    } else {
                        Self::get_node_style(&tree_item.node)
                    },
                )]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title("Document Tree")
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        let mut state = ListState::default();
        state.select(Some(self.selected_index));

        frame.render_stateful_widget(list, area, &mut state);
    }

    fn get_node_style(node: &Node) -> Style {
        match node {
            Node::Heading(_) => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            Node::List(_) => Style::default().fg(Color::Green),
            Node::Code(_) | Node::CodeInline(_) => Style::default().fg(Color::Cyan),
            Node::Link(_) | Node::LinkRef(_) => Style::default().fg(Color::Magenta),
            Node::Strong(_) => Style::default().add_modifier(Modifier::BOLD),
            Node::Emphasis(_) => Style::default().add_modifier(Modifier::ITALIC),
            Node::Image(_) | Node::ImageRef(_) => Style::default().fg(Color::Yellow),
            Node::Math(_) | Node::MathInline(_) => Style::default().fg(Color::Red),
            Node::Blockquote(_) => Style::default().fg(Color::LightBlue),
            Node::HorizontalRule(_) => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(Color::Gray),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_markdown::{Heading, Text};

    fn create_test_heading() -> Node {
        Node::Heading(Heading {
            depth: 1,
            values: vec![Node::Text(Text {
                value: "Test Heading".to_string(),
                position: None,
            })],
            position: None,
        })
    }

    fn create_test_text() -> Node {
        Node::Text(Text {
            value: "Test text content".to_string(),
            position: None,
        })
    }

    #[test]
    fn test_tree_item_creation() {
        let node = create_test_heading();
        let item = TreeItem::new(node, 0, 0);

        assert_eq!(item.depth, 0);
        assert_eq!(item.index, 0);
        assert!(item.has_children);
        assert_eq!(item.display_text, "H1 Test Heading");
    }

    #[test]
    fn test_tree_view_creation() {
        let nodes = vec![create_test_heading(), create_test_text()];
        let tree_view = TreeView::new(nodes);

        assert_eq!(tree_view.items.len(), 2);
        assert_eq!(tree_view.selected_index, 0);
    }

    #[test]
    fn test_navigation() {
        let nodes = vec![create_test_heading(), create_test_text()];
        let mut tree_view = TreeView::new(nodes);

        tree_view.move_down();
        assert_eq!(tree_view.selected_index, 1);

        tree_view.move_up();
        assert_eq!(tree_view.selected_index, 0);
    }

    #[test]
    fn test_toggle_expand() {
        let nodes = vec![create_test_heading()];
        let mut tree_view = TreeView::new(nodes);

        let initial_count = tree_view.items.len();
        tree_view.toggle_expand();
        let collapsed_count = tree_view.items.len();

        assert!(collapsed_count > initial_count);
    }

    #[test]
    fn test_get_selected_node() {
        let nodes = vec![create_test_text()];
        let tree_view = TreeView::new(nodes);

        let selected = tree_view.get_selected_node();
        assert!(selected.is_some());

        if let Some(Node::Text(text)) = selected {
            assert_eq!(text.value, "Test text content");
        } else {
            panic!("Expected text node");
        }
    }

    #[test]
    fn test_has_children_comprehensive() {
        use mq_markdown::{
            Blockquote, Code, Delete, Emphasis, Footnote, Fragment, Image, Link, List,
            MdxJsxFlowElement, MdxJsxTextElement, Strong, TableCell, TableRow, Url,
        };

        // Test nodes that have children
        let heading_with_children = create_test_heading();
        assert!(TreeItem::has_children(&heading_with_children));

        let list_with_items = Node::List(List {
            ordered: false,
            values: vec![create_test_text()],
            index: 0,
            level: 0,
            checked: None,
            position: None,
        });
        assert!(TreeItem::has_children(&list_with_items));

        let blockquote_with_content = Node::Blockquote(Blockquote {
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&blockquote_with_content));

        let strong_with_content = Node::Strong(Strong {
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&strong_with_content));

        let emphasis_with_content = Node::Emphasis(Emphasis {
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&emphasis_with_content));

        let link_with_content = Node::Link(Link {
            url: Url::new("https://example.com".to_string()),
            title: None,
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&link_with_content));

        let delete_with_content = Node::Delete(Delete {
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&delete_with_content));

        let fragment_with_content = Node::Fragment(Fragment {
            values: vec![create_test_text()],
        });
        assert!(TreeItem::has_children(&fragment_with_content));

        let footnote_with_content = Node::Footnote(Footnote {
            ident: "note1".to_string(),
            values: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&footnote_with_content));

        let table_row_with_cells = Node::TableRow(TableRow {
            values: vec![Node::TableCell(TableCell {
                values: vec![],
                column: 0,
                row: 0,
                last_cell_in_row: false,
                last_cell_of_in_table: false,
                position: None,
            })],
            position: None,
        });
        assert!(TreeItem::has_children(&table_row_with_cells));

        let table_cell_with_content = Node::TableCell(TableCell {
            values: vec![create_test_text()],
            column: 0,
            row: 0,
            last_cell_in_row: false,
            last_cell_of_in_table: false,
            position: None,
        });
        assert!(TreeItem::has_children(&table_cell_with_content));

        let mdx_flow_with_children = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            name: Some("div".into()),
            attributes: vec![],
            children: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&mdx_flow_with_children));

        let mdx_text_with_children = Node::MdxJsxTextElement(MdxJsxTextElement {
            name: Some("span".into()),
            attributes: vec![],
            children: vec![create_test_text()],
            position: None,
        });
        assert!(TreeItem::has_children(&mdx_text_with_children));

        // Test nodes that don't have children
        let text = create_test_text();
        assert!(!TreeItem::has_children(&text));

        let empty_list = Node::List(List {
            ordered: false,
            values: vec![],
            index: 0,
            level: 0,
            checked: None,
            position: None,
        });
        assert!(!TreeItem::has_children(&empty_list));

        let empty_blockquote = Node::Blockquote(Blockquote {
            values: vec![],
            position: None,
        });
        assert!(!TreeItem::has_children(&empty_blockquote));

        let code = Node::Code(Code {
            lang: Some("rust".to_string()),
            value: "fn main() {}".to_string(),
            position: None,
            meta: None,
            fence: false,
        });
        assert!(!TreeItem::has_children(&code));

        let image = Node::Image(Image {
            url: "image.jpg".to_string(),
            alt: "Alt text".to_string(),
            title: None,
            position: None,
        });
        assert!(!TreeItem::has_children(&image));

        let empty_mdx_flow = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            name: Some("div".into()),
            attributes: vec![],
            children: vec![],
            position: None,
        });
        assert!(!TreeItem::has_children(&empty_mdx_flow));

        let empty = Node::Empty;
        assert!(!TreeItem::has_children(&empty));
    }

    #[test]
    fn test_create_display_text_comprehensive() {
        use mq_markdown::{
            Blockquote, Break, Code, CodeInline, Definition, Delete, Emphasis, Footnote,
            FootnoteRef, Fragment, HorizontalRule, Html, Image, ImageRef, Link, LinkRef, List,
            Math, MathInline, MdxJsxFlowElement, MdxJsxTextElement, Strong, TableCell, TableRow,
            Toml, Url, Yaml,
        };

        // Test heading
        let heading = create_test_heading();
        assert_eq!(TreeItem::create_display_text(&heading), "H1 Test Heading");

        // Test text
        let text = create_test_text();
        assert_eq!(
            TreeItem::create_display_text(&text),
            "Text: Test text content"
        );

        // Test long text (truncation)
        let long_text = Node::Text(Text {
            value: "This is a very long text that should be truncated when displayed".to_string(),
            position: None,
        });
        let display = TreeItem::create_display_text(&long_text);
        assert!(display.starts_with("Text: This is a very long text that should be tru"));
        assert!(display.ends_with("..."));

        // Test unordered list
        let unordered_list = Node::List(List {
            ordered: false,
            values: vec![create_test_text(), create_test_text()],
            index: 0,
            level: 0,
            checked: None,
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&unordered_list),
            "Unordered List (2 items)"
        );

        // Test ordered list
        let ordered_list = Node::List(List {
            ordered: true,
            values: vec![create_test_text()],
            index: 0,
            level: 0,
            checked: None,
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&ordered_list),
            "Ordered List (1 items)"
        );

        // Test code block
        let code = Node::Code(Code {
            lang: Some("rust".to_string()),
            value: "fn main() {}".to_string(),
            position: None,
            meta: None,
            fence: false,
        });
        assert_eq!(TreeItem::create_display_text(&code), "Code Block (rust)");

        let code_no_lang = Node::Code(Code {
            lang: None,
            value: "some code".to_string(),
            position: None,
            meta: None,
            fence: false,
        });
        assert_eq!(
            TreeItem::create_display_text(&code_no_lang),
            "Code Block (text)"
        );

        // Test blockquote
        let blockquote = Node::Blockquote(Blockquote {
            values: vec![create_test_text()],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&blockquote), "Blockquote");

        // Test strong
        let strong = Node::Strong(Strong {
            values: vec![create_test_text()],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&strong), "Strong");

        // Test emphasis
        let emphasis = Node::Emphasis(Emphasis {
            values: vec![create_test_text()],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&emphasis), "Emphasis");

        // Test link
        let link = Node::Link(Link {
            url: Url::new("https://example.com".to_string()),
            title: None,
            values: vec![Node::Text(Text {
                value: "Example Link".to_string(),
                position: None,
            })],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&link), "Link: Example Link");

        // Test image
        let image = Node::Image(Image {
            url: "image.jpg".to_string(),
            alt: "Alt text".to_string(),
            title: None,
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&image), "Image: Alt text");

        // Test horizontal rule
        let hr = Node::HorizontalRule(HorizontalRule { position: None });
        assert_eq!(TreeItem::create_display_text(&hr), "Horizontal Rule");

        // Test table components - removing TableHeader test as it doesn't have values field

        let table_row = Node::TableRow(TableRow {
            values: vec![],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&table_row), "Table Row");

        let table_cell = Node::TableCell(TableCell {
            values: vec![],
            column: 0,
            row: 0,
            last_cell_in_row: false,
            last_cell_of_in_table: false,
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&table_cell), "Table Cell");

        // Test break
        let br = Node::Break(Break { position: None });
        assert_eq!(TreeItem::create_display_text(&br), "Line Break");

        // Test HTML
        let html = Node::Html(Html {
            value: "<div>content</div>".to_string(),
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&html),
            "HTML: <div>content</div>"
        );

        // Test math
        let math = Node::Math(Math {
            value: "x = y + z".to_string(),
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&math), "Math: x = y + z");

        // Test inline math
        let math_inline = Node::MathInline(MathInline {
            value: "x^2".to_string().into(),
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&math_inline),
            "Inline Math: x^2"
        );

        // Test inline code
        let code_inline = Node::CodeInline(CodeInline {
            value: "println!".to_string().into(),
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&code_inline),
            "Inline Code: println!"
        );

        // Test delete (strikethrough)
        let delete = Node::Delete(Delete {
            values: vec![],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&delete), "Strikethrough");

        // Test YAML
        let yaml = Node::Yaml(Yaml {
            value: "key: value".to_string(),
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&yaml), "YAML: key: value");

        // Test TOML
        let toml = Node::Toml(Toml {
            value: "[section]".to_string(),
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&toml), "TOML: [section]");

        // Test Fragment
        let fragment = Node::Fragment(Fragment { values: vec![] });
        assert_eq!(TreeItem::create_display_text(&fragment), "Fragment");

        // Test Footnote
        let footnote = Node::Footnote(Footnote {
            ident: "note1".to_string(),
            values: vec![],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&footnote), "Footnote");

        // Test FootnoteRef
        let footnote_ref = Node::FootnoteRef(FootnoteRef {
            ident: "note1".to_string(),
            label: Some("note1".to_string()),
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&footnote_ref),
            "Footnote Ref: note1"
        );

        // Test Definition
        let definition = Node::Definition(Definition {
            ident: "def1".to_string(),
            label: Some("def1".to_string()),
            url: Url::new("url".to_string()),
            title: None,
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&definition),
            "Definition: def1"
        );

        // Test ImageRef
        let image_ref = Node::ImageRef(ImageRef {
            ident: "img1".to_string(),
            label: Some("img1".to_string()),
            alt: "alt".to_string(),
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&image_ref), "Image Ref: img1");

        // Test LinkRef
        let link_ref = Node::LinkRef(LinkRef {
            ident: "link1".to_string(),
            label: Some("link1".to_string()),
            values: vec![],
            position: None,
        });
        assert_eq!(TreeItem::create_display_text(&link_ref), "Link Ref: link1");

        // Test MDX JSX Flow Element
        let mdx_flow = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            name: Some("div".into()),
            attributes: vec![],
            children: vec![],
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&mdx_flow),
            "MDX JSX Element: div"
        );

        let mdx_flow_no_name = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            name: None,
            attributes: vec![],
            children: vec![],
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&mdx_flow_no_name),
            "MDX JSX Element: element"
        );

        // Test MDX JSX Text Element
        let mdx_text = Node::MdxJsxTextElement(MdxJsxTextElement {
            name: Some("span".to_string().into()),
            attributes: vec![],
            children: vec![],
            position: None,
        });
        assert_eq!(
            TreeItem::create_display_text(&mdx_text),
            "MDX JSX Text: span"
        );

        // Test Empty
        let empty = Node::Empty;
        assert_eq!(TreeItem::create_display_text(&empty), "Empty");
    }

    #[test]
    fn test_get_children() {
        use mq_markdown::{
            Blockquote, Code, Delete, Emphasis, Footnote, Fragment, Image, Link, List,
            MdxJsxFlowElement, MdxJsxTextElement, Strong, TableCell, TableRow, Url,
        };

        // Test heading with children
        let heading = create_test_heading();
        let item = TreeItem::new(heading.clone(), 0, 0);
        let children = item.get_children();
        assert_eq!(children.len(), 1);
        if let Node::Text(text) = &children[0] {
            assert_eq!(text.value, "Test Heading");
        }

        // Test list with items
        let list_node = Node::List(List {
            ordered: false,
            values: vec![create_test_text(), create_test_text()],
            index: 0,
            level: 0,
            checked: None,
            position: None,
        });
        let list_item = TreeItem::new(list_node, 0, 0);
        let list_children = list_item.get_children();
        assert_eq!(list_children.len(), 2);

        // Test blockquote with content
        let blockquote = Node::Blockquote(Blockquote {
            values: vec![create_test_text()],
            position: None,
        });
        let blockquote_item = TreeItem::new(blockquote, 0, 0);
        let blockquote_children = blockquote_item.get_children();
        assert_eq!(blockquote_children.len(), 1);

        // Test strong with content
        let strong = Node::Strong(Strong {
            values: vec![create_test_text()],
            position: None,
        });
        let strong_item = TreeItem::new(strong, 0, 0);
        let strong_children = strong_item.get_children();
        assert_eq!(strong_children.len(), 1);

        // Test emphasis with content
        let emphasis = Node::Emphasis(Emphasis {
            values: vec![create_test_text()],
            position: None,
        });
        let emphasis_item = TreeItem::new(emphasis, 0, 0);
        let emphasis_children = emphasis_item.get_children();
        assert_eq!(emphasis_children.len(), 1);

        // Test link with content
        let link = Node::Link(Link {
            url: Url::new("https://example.com".to_string()),
            title: None,
            values: vec![create_test_text()],
            position: None,
        });
        let link_item = TreeItem::new(link, 0, 0);
        let link_children = link_item.get_children();
        assert_eq!(link_children.len(), 1);

        // Test delete with content
        let delete = Node::Delete(Delete {
            values: vec![create_test_text()],
            position: None,
        });
        let delete_item = TreeItem::new(delete, 0, 0);
        let delete_children = delete_item.get_children();
        assert_eq!(delete_children.len(), 1);

        // Test fragment with content
        let fragment = Node::Fragment(Fragment {
            values: vec![create_test_text(), create_test_text()],
        });
        let fragment_item = TreeItem::new(fragment, 0, 0);
        let fragment_children = fragment_item.get_children();
        assert_eq!(fragment_children.len(), 2);

        // Test footnote with content
        let footnote = Node::Footnote(Footnote {
            ident: "note1".to_string(),
            values: vec![create_test_text()],
            position: None,
        });
        let footnote_item = TreeItem::new(footnote, 0, 0);
        let footnote_children = footnote_item.get_children();
        assert_eq!(footnote_children.len(), 1);

        // Test table row with cells
        let table_row = Node::TableRow(TableRow {
            values: vec![Node::TableCell(TableCell {
                values: vec![create_test_text()],
                column: 0,
                row: 0,
                last_cell_in_row: false,
                last_cell_of_in_table: false,
                position: None,
            })],
            position: None,
        });
        let table_row_item = TreeItem::new(table_row, 0, 0);
        let table_row_children = table_row_item.get_children();
        assert_eq!(table_row_children.len(), 1);

        // Test table cell with content
        let table_cell = Node::TableCell(TableCell {
            values: vec![create_test_text()],
            column: 0,
            row: 0,
            last_cell_in_row: false,
            last_cell_of_in_table: false,
            position: None,
        });
        let table_cell_item = TreeItem::new(table_cell, 0, 0);
        let table_cell_children = table_cell_item.get_children();
        assert_eq!(table_cell_children.len(), 1);

        // Test MDX JSX Flow Element with children
        let mdx_flow = Node::MdxJsxFlowElement(MdxJsxFlowElement {
            name: Some("div".into()),
            attributes: vec![],
            children: vec![create_test_text()],
            position: None,
        });
        let mdx_flow_item = TreeItem::new(mdx_flow, 0, 0);
        let mdx_flow_children = mdx_flow_item.get_children();
        assert_eq!(mdx_flow_children.len(), 1);

        // Test MDX JSX Text Element with children
        let mdx_text = Node::MdxJsxTextElement(MdxJsxTextElement {
            name: Some("span".to_string().into()),
            attributes: vec![],
            children: vec![create_test_text()],
            position: None,
        });
        let mdx_text_item = TreeItem::new(mdx_text, 0, 0);
        let mdx_text_children = mdx_text_item.get_children();
        assert_eq!(mdx_text_children.len(), 1);

        // Test nodes with no children
        let text = create_test_text();
        let text_item = TreeItem::new(text, 0, 0);
        let text_children = text_item.get_children();
        assert!(text_children.is_empty());

        let code = Node::Code(Code {
            lang: Some("rust".to_string()),
            value: "fn main() {}".to_string(),
            position: None,
            meta: None,
            fence: false,
        });
        let code_item = TreeItem::new(code, 0, 0);
        let code_children = code_item.get_children();
        assert!(code_children.is_empty());

        let image = Node::Image(Image {
            url: "image.jpg".to_string(),
            alt: "Alt text".to_string(),
            title: None,
            position: None,
        });
        let image_item = TreeItem::new(image, 0, 0);
        let image_children = image_item.get_children();
        assert!(image_children.is_empty());

        let empty = Node::Empty;
        let empty_item = TreeItem::new(empty, 0, 0);
        let empty_children = empty_item.get_children();
        assert!(empty_children.is_empty());
    }

    #[test]
    fn test_render() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        // Create a simple tree view with some nodes
        let nodes = vec![
            create_test_heading(),
            create_test_text(),
            Node::List(mq_markdown::List {
                ordered: false,
                values: vec![create_test_text()],
                index: 0,
                level: 0,
                checked: None,
                position: None,
            }),
        ];
        let tree_view = TreeView::new(nodes);

        // Create a test terminal
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test rendering
        let result = terminal.draw(|frame| {
            let area = Rect::new(0, 0, 80, 10);
            tree_view.render(frame, area);
        });

        assert!(result.is_ok());

        // Test that the buffer contains expected elements
        let buffer = terminal.backend().buffer().clone();

        // Check that content was rendered (buffer has non-empty cells)
        let has_content = buffer
            .content()
            .iter()
            .any(|cell| !cell.symbol().is_empty());
        assert!(has_content, "Tree view should have rendered content");

        // The tree view should have some items
        assert!(
            !tree_view.items().is_empty(),
            "Tree view should have items to render"
        );
    }

    #[test]
    fn test_render_with_expanded_items() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        // Create a tree view with expandable content
        let nodes = vec![create_test_heading()];
        let mut tree_view = TreeView::new(nodes);

        // Expand the first item
        tree_view.toggle_expand();

        // Create a test terminal
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test rendering with expanded items
        let result = terminal.draw(|frame| {
            let area = Rect::new(0, 0, 80, 10);
            tree_view.render(frame, area);
        });

        assert!(result.is_ok());

        // Verify that we have more items after expansion
        assert!(tree_view.items().len() > 1, "Should have expanded items");

        // Check that content was rendered
        let buffer = terminal.backend().buffer().clone();
        let has_content = buffer
            .content()
            .iter()
            .any(|cell| !cell.symbol().is_empty());

        // This test mainly verifies that rendering doesn't panic and produces output
        assert!(has_content, "Should have rendered content");
    }
}
