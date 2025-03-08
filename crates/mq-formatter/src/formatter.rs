use std::sync::{Arc, LazyLock};

use rustc_hash::FxHashSet;

#[cfg(target_os = "windows")]
const NEW_LINE: &str = "\r\n";
#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
const NEW_LINE: &str = "\n";

#[derive(Clone, Debug, Default)]
pub struct Formatter {
    config: FormatterConfig,
    output: String,
}

#[derive(Clone, Debug)]
pub struct FormatterConfig {
    pub indent_width: usize,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self { indent_width: 2 }
    }
}

static IGNORE_TRIVIA_KIND: LazyLock<FxHashSet<mq_lang::CstNodeKind>> = LazyLock::new(|| {
    let mut set = FxHashSet::default();
    set.insert(mq_lang::CstNodeKind::Token);
    set
});

impl Formatter {
    pub fn new(config: Option<FormatterConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            output: String::new(),
        }
    }

    pub fn format(&mut self, code: &str) -> Result<String, mq_lang::CstErrorReporter> {
        if code.is_empty() {
            return Ok(String::new());
        }

        let (nodes, errors) = mq_lang::parse_recovery(code);

        if errors.has_errors() {
            return Err(errors);
        }

        self.format_with_cst(nodes)
    }

    pub fn format_with_cst(
        &mut self,
        nodes: Vec<Arc<mq_lang::CstNode>>,
    ) -> Result<String, mq_lang::CstErrorReporter> {
        for node in &nodes {
            self.visit_node(Arc::clone(node), 0);
        }

        Ok(self.output.clone())
    }

    fn visit_node(&mut self, node: Arc<mq_lang::CstNode>, indent_level: usize) {
        let has_leading_new_line = node.has_new_line();

        let indent_level_consider_new_line = if has_leading_new_line {
            indent_level
        } else {
            0
        };

        if !IGNORE_TRIVIA_KIND.contains(&node.kind) {
            self.append_leading_trivia(&node, indent_level_consider_new_line);
        }

        match &node.kind {
            mq_lang::CstNodeKind::Call => self.format_call(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Def => {
                self.format_expr(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Foreach => {
                self.format_expr(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Ident => self.format_ident(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::If => self.format_if(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Include => {
                self.format_include(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Let => self.format_let(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Elif => self.format_elif(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Else => self.format_else(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Token => {
                self.append_punctuations(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Selector => {
                self.format_selector(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Self_ => self.format_self(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Literal => {
                self.append_literal(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::While => {
                self.format_expr(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Until => {
                self.format_expr(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Eof => {}
        }

        self.append_trailing_trivia(&node, indent_level);
    }

    fn format_include(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        node.children.iter().for_each(|child| {
            self.visit_node(Arc::clone(child), indent_level);
        });
    }

    fn format_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        let is_prev_pipe = self.is_prev_pipe();

        if node.has_new_line() {
            self.append_indent(indent_level);
        }
        self.output.push_str(&node.to_string());
        self.append_space();

        let expr_index = node
            .children
            .iter()
            .position(|c| {
                c.token
                    .as_ref()
                    .map(|token| matches!(token.kind, mq_lang::TokenKind::Colon))
                    .unwrap_or(false)
            })
            .unwrap();

        node.children.iter().take(expr_index).for_each(|child| {
            self.visit_node(
                Arc::clone(child),
                if node.has_new_line() {
                    block_indent_level + 1
                } else {
                    block_indent_level
                },
            );
        });

        let mut expr_nodes = node.children.iter().skip(expr_index).peekable();
        let colon_node = expr_nodes.next().unwrap();

        self.visit_node(Arc::clone(colon_node), block_indent_level + 1);

        if !expr_nodes.peek().unwrap().has_new_line() {
            self.append_space();
        }

        let block_indent_level = if is_prev_pipe {
            block_indent_level + 2
        } else {
            block_indent_level + 1
        };

        expr_nodes.for_each(|child| {
            self.visit_node(Arc::clone(child), block_indent_level);
        });
    }

    fn format_let(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        node.children.iter().for_each(|child| {
            self.visit_node(Arc::clone(child), indent_level);
        });
    }

    fn format_call(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        node.children.iter().for_each(|child| {
            self.visit_node(
                Arc::clone(child),
                if node.has_new_line() {
                    indent_level + 1
                } else {
                    indent_level
                },
            );
        });
    }

    fn format_if(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        if let [l_param, cond, r_param, then_colon, then_expr, rest @ ..] = node.children.as_slice()
        {
            self.visit_node(Arc::clone(l_param), 0);
            self.visit_node(Arc::clone(cond), 0);
            self.visit_node(Arc::clone(r_param), 0);
            self.visit_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.visit_node(Arc::clone(then_expr), indent_level + 1);

            for child in rest {
                self.visit_node(Arc::clone(child), indent_level);
            }
        }
    }

    fn format_elif(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        if let [l_param, cond, r_param, then_colon, then_expr] = node.children.as_slice() {
            self.visit_node(Arc::clone(l_param), 0);
            self.visit_node(Arc::clone(cond), 0);
            self.visit_node(Arc::clone(r_param), 0);
            self.visit_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.visit_node(Arc::clone(then_expr), indent_level + 1);
        }
    }

    fn format_else(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        if let [then_colon, then_expr] = node.children.as_slice() {
            self.visit_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.visit_node(Arc::clone(then_expr), indent_level + 1);
        }
    }

    fn format_ident(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
    }

    fn append_leading_trivia(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        for trivia in &node.leading_trivia {
            match trivia {
                mq_lang::CstTrivia::Whitespace(_) => {}
                comment @ mq_lang::CstTrivia::Comment(_) => {
                    self.append_indent(indent_level);
                    self.output.push_str(&comment.to_string());
                }
                mq_lang::CstTrivia::NewLine => {
                    self.output.push('\n');
                }
                _ => {}
            }
        }
    }

    fn append_trailing_trivia(&mut self, node: &Arc<mq_lang::CstNode>, _indent_level: usize) {
        for trivia in &node.trailing_trivia {
            match trivia {
                mq_lang::CstTrivia::Whitespace(_) => {}
                mq_lang::CstTrivia::Tab(_) => {}
                mq_lang::CstTrivia::NewLine => {}
                _ => {}
            }
        }
    }

    fn append_indent(&mut self, level: usize) {
        self.output
            .push_str(&" ".repeat(level * self.config.indent_width));
    }

    fn append_literal(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Literal,
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            match &token.kind {
                mq_lang::TokenKind::StringLiteral(s) => self.output.push_str(&format!(
                    "\"{}\"",
                    &s.replace("\"", "\\\"")
                        .replace("\\n", "\\\\n")
                        .replace("\\t", "\\\\t")
                        .replace("\\r", "\\\\r")
                )),
                mq_lang::TokenKind::NumberLiteral(n) => self.output.push_str(&n.to_string()),
                mq_lang::TokenKind::BoolLiteral(b) => self.output.push_str(&b.to_string()),
                mq_lang::TokenKind::None => self.output.push_str(&token.to_string()),
                _ => {}
            }
        }
    }

    fn format_selector(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Selector,
            token: Some(token),
            children,
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.output.push_str(&token.to_string());

            children.iter().for_each(|child| {
                self.visit_node(Arc::clone(child), indent_level);
            });
        }
    }

    fn format_self(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Self_,
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.output.push_str(&token.to_string());
        }
    }

    fn append_punctuations(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Token,
            token: Some(token),
            ..
        } = &**node
        {
            match token.kind {
                mq_lang::TokenKind::Comma => {
                    if node.has_new_line() {
                        self.append_indent(indent_level);
                        self.output.push_str(&format!("{}", token))
                    } else {
                        self.output.push_str(&format!("{} ", token))
                    }
                }
                mq_lang::TokenKind::Colon => self.output.push_str(&format!("{}", token)),
                mq_lang::TokenKind::Equal => self.output.push_str(&format!(" {} ", token)),
                mq_lang::TokenKind::Pipe => {
                    if node.has_new_line() {
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.output.push_str(&format!("{} ", token))
                    } else {
                        self.output.push_str(&format!(" {} ", token))
                    }
                }
                _ => self.output.push_str(&token.to_string()),
            }
        }
    }

    fn append_space(&mut self) {
        self.output.push(' ');
    }

    fn is_prev_pipe(&self) -> bool {
        self.output.ends_with("| ")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("", "")]
    #[case(
        "if(test):
        test
        else:
        test2",
        "if (test):
  test
else:
  test2"
    )]
    #[case(
        "def name(test):
        test | test2;",
        "def name(test):
  test | test2;"
    )]
    #[case(
        "foreach(x,array(1, 2, 3)):
        add(x, 1);",
        "foreach (x, array(1, 2, 3)):
  add(x, 1);"
    )]
    #[case(
        "def test():
        test1
        |test2
        |test3",
        "def test():
  test1
  | test2
  | test3"
    )]
    #[case("test()?|test2()", "test()? | test2()")]
    #[case(
        "if(test):
        test
        elif(test2):
        test2
        else:
        test3",
        "if (test):
  test
elif (test2):
  test2
else:
  test3"
    )]
    #[case::if_(
        "if(test):
        test
        elif(test2):
        test2
        else:
        test3",
        "if (test):
  test
elif (test2):
  test2
else:
  test3"
    )]
    #[case::if_(
        "if(test):
        test
        else:
        test2",
        "if (test):
  test
else:
  test2"
    )]
    #[case::one_line("if(test): test else: test2", "if (test): test else: test2")]
    #[case::one_line(
        "if(test): test elif(test2): test2 else: test3",
        "if (test): test elif (test2): test2 else: test3"
    )]
    #[case::foreach_one_line(
        "foreach(x,array(1,2,3)):add(x,1);",
        "foreach (x, array(1, 2, 3)): add(x, 1);"
    )]
    #[case::foreach_one_line(
        "foreach(x,array(1,2,3)):add(x,1);add(1,2);",
        "foreach (x, array(1, 2, 3)): add(x, 1);add(1, 2);"
    )]
    #[case::foreach_one_line(".[]|upcase()", ".[] | upcase()")]
    #[case::test(
        "# Sample
def hello_world():
  add(\" Hello World\")?;
select(or(.[],.code,.h))|upcase()|hello_world()",
        "# Sample
def hello_world():
  add(\" Hello World\")?;
select(or(.[], .code, .h)) | upcase() | hello_world()"
    )]
    #[case::test(
        ".h
| let link = to_link(add(\"#\", to_text(self)), to_text(self));
| if (eq(to_md_name(), \"h1\")):
    to_md_list(link, 1)
  elif (eq(to_md_name(),\"h2\")):
    to_md_list(link, 2)
  elif (eq(to_md_name(), \"h3\")):
    to_md_list(link, 3)
  elif (eq(to_md_name(), \"h4\")):
    to_md_list(link, 4)
  elif (eq(to_md_name(), \"h5\")):
    to_md_list(link, 5)
  else:
    None",
        ".h
| let link = to_link(add(\"#\", to_text(self)), to_text(self));
| if (eq(to_md_name(), \"h1\")):
  to_md_list(link, 1)
elif (eq(to_md_name(), \"h2\")):
  to_md_list(link, 2)
elif (eq(to_md_name(), \"h3\")):
  to_md_list(link, 3)
elif (eq(to_md_name(), \"h4\")):
  to_md_list(link, 4)
elif (eq(to_md_name(), \"h5\")):
  to_md_list(link, 5)
else:
  None"
    )]
    #[case::test(
        "def snake_to_camel(x):
  let words = split(x, \"_\")
  | foreach (word, words):
  let first_char = upcase(first(word))
  | let rest_str = downcase(slice(word, 1, len(word)))
  | add(first_char, rest_str);
  | join(\"\");
| snake_to_camel()",
        "def snake_to_camel(x):
  let words = split(x, \"_\")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | add(first_char, rest_str);
  | join(\"\");
| snake_to_camel()"
    )]
    #[case::test(
        "def snake_to_camel(x): let words = split(x, \"_\") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join(\"\");| snake_to_camel()",
        "def snake_to_camel(x): let words = split(x, \"_\") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join(\"\"); | snake_to_camel()"
    )]
    #[case::string("let test = \"test\"", "let test = \"test\"")]
    fn test_format(#[case] code: &str, #[case] expected: &str) {
        let result = Formatter::new(None).format(code);
        assert_eq!(result.unwrap(), expected);
    }
}
