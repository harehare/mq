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
            is_expanded: has_children,
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

        assert_eq!(tree_view.items.len(), 3); // Heading + its child text + standalone text
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

        assert!(collapsed_count < initial_count);
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
    fn test_has_children_detection() {
        let heading = create_test_heading();
        let text = create_test_text();

        assert!(TreeItem::has_children(&heading));
        assert!(!TreeItem::has_children(&text));
    }

    #[test]
    fn test_display_text_creation() {
        let heading = create_test_heading();
        let text = create_test_text();

        assert_eq!(TreeItem::create_display_text(&heading), "H1 Test Heading");
        assert_eq!(
            TreeItem::create_display_text(&text),
            "Text: Test text content"
        );
    }
}
