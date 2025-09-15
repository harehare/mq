use std::sync::Arc;

#[allow(dead_code)]
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
            self.format_node(Arc::clone(node), 0);
        }

        let mut result = String::with_capacity(self.output.len());

        for line in self.output.lines() {
            result.push_str(line.trim_end());
            result.push('\n');
        }

        if result.ends_with('\n') && !self.output.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn format_node(&mut self, node: Arc<mq_lang::CstNode>, indent_level: usize) {
        let has_leading_new_line = node.has_new_line();

        let indent_level_consider_new_line = if has_leading_new_line {
            indent_level
        } else {
            0
        };

        if !matches!(
            node.kind,
            mq_lang::CstNodeKind::Token
                | mq_lang::CstNodeKind::BinaryOp(_)
                | mq_lang::CstNodeKind::End
        ) {
            self.append_leading_trivia(&node, indent_level_consider_new_line);
        }

        match &node.kind {
            mq_lang::CstNodeKind::Array => {
                self.format_array(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Dict => {
                self.format_dict(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::BinaryOp(_) => {
                self.format_binary_op(&node, indent_level);
            }
            mq_lang::CstNodeKind::UnaryOp(_) => {
                self.format_unary_op(&node, indent_level);
            }
            mq_lang::CstNodeKind::Group => {
                self.format_group(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Call => self.format_call(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Def
            | mq_lang::CstNodeKind::Foreach
            | mq_lang::CstNodeKind::While
            | mq_lang::CstNodeKind::Until
            | mq_lang::CstNodeKind::Fn => self.format_expr(
                &node,
                indent_level_consider_new_line,
                indent_level,
                !matches!(node.kind, mq_lang::CstNodeKind::Fn),
            ),
            mq_lang::CstNodeKind::Eof => {}
            mq_lang::CstNodeKind::Elif => self.format_elif(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Else => self.format_else(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Ident => self.format_ident(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::If => {
                self.format_if(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Include => {
                self.format_include(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::InterpolatedString => {
                self.append_interpolated_string(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Let => {
                self.format_let(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Literal => {
                self.append_literal(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Env => self.append_env(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Self_
            | mq_lang::CstNodeKind::Break
            | mq_lang::CstNodeKind::Continue => {
                self.format_keyword(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Selector => {
                self.format_selector(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::Token => self.append_token(&node, indent_level_consider_new_line),
        }
    }

    fn format_include(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        node.children.iter().for_each(|child| {
            self.format_node(Arc::clone(child), indent_level);
        });
    }

    fn format_array(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let len = node.children.len();

        if len == 0 {
            return;
        }

        let indent_adjustment = if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        };

        let is_multiline = node.children[1].has_new_line();

        for child in &node.children[..len.saturating_sub(1)] {
            self.format_node(Arc::clone(child), indent_level + indent_adjustment + 1);
        }

        if let Some(last) = node.children.last() {
            if is_multiline {
                self.append_newline();
                self.append_indent(indent_level + indent_adjustment);
            }

            self.format_node(Arc::clone(last), indent_level + indent_adjustment);
        }
    }

    fn format_group(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        node.children.iter().for_each(|child| {
            self.format_node(Arc::clone(child), indent_level);
        });
    }

    fn format_dict(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let indent_adjustment = if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        };

        let len = node.children.len();
        if len == 0 {
            return;
        }

        // Format LBrace
        self.format_node(
            Arc::clone(&node.children[0]),
            indent_level + indent_adjustment,
        );

        // Early return if only braces exist
        if len == 2 {
            self.format_node(
                Arc::clone(&node.children[1]),
                indent_level + indent_adjustment,
            );
            return;
        }

        let is_multiline = node.children[1].has_new_line();

        let mut i = 1;
        while i < len - 1 {
            let key = &node.children[i];
            let colon = node.children.get(i + 1);
            let value = node.children.get(i + 2);

            // Defensive: ensure we have key: value
            if let (Some(colon), Some(value)) = (colon, value) {
                if key.has_new_line() {
                    self.format_node(Arc::clone(key), indent_level + indent_adjustment + 1);
                } else {
                    self.format_node(Arc::clone(key), 0);
                }
                self.format_node(Arc::clone(colon), 0);
                self.append_space();
                self.format_node(Arc::clone(value), 0);
                i += 3;
            } else {
                self.format_node(Arc::clone(key), 0);
                i += 1;
            }

            // Handle comma if present
            if i < len - 1 {
                if let Some(token) = node.children[i].token.as_ref() {
                    if matches!(token.kind, mq_lang::TokenKind::Comma) {
                        self.format_node(Arc::clone(&node.children[i]), 0);
                        i += 1;
                    }
                }
            }
        }

        // Format RBrace
        if let Some(rbrace) = node.children.last() {
            if is_multiline {
                self.append_newline();
                self.append_indent(indent_level + indent_adjustment);
            }

            self.format_node(Arc::clone(rbrace), indent_level + indent_adjustment);
        }
    }

    fn format_binary_op(&mut self, node: &Arc<mq_lang::CstNode>, block_indent_level: usize) {
        match node.binary_op() {
            Some((left, right)) => {
                self.format_node(left, block_indent_level);

                match &**node {
                    mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::BinaryOp(mq_lang::CstBinaryOp::RangeOp),
                        token: Some(token),
                        ..
                    } => {
                        self.append_leading_trivia(node, block_indent_level);

                        if node.has_new_line() {
                            self.append_indent(block_indent_level);
                        }
                        self.output.push_str(format!("{}", token).as_str());
                    }
                    mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::BinaryOp(_),
                        token: Some(token),
                        ..
                    } => {
                        self.append_leading_trivia(node, block_indent_level);

                        if node.has_new_line() {
                            self.append_indent(block_indent_level);
                        }

                        self.output.push_str(format!(" {} ", token).as_str());
                    }
                    _ => unreachable!(),
                }

                self.format_node(right, block_indent_level);
            }
            _ => unreachable!(),
        }
    }

    fn format_unary_op(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::UnaryOp(op),
            token: Some(token),
            ..
        } = &**node
        {
            if node.has_new_line() {
                self.append_indent(indent_level);
            }
            self.output.push_str(&token.to_string());

            match op {
                mq_lang::CstUnaryOp::Not | mq_lang::CstUnaryOp::Negate => {
                    self.format_node(Arc::clone(&node.children[0]), indent_level);
                }
            }
        } else {
            unreachable!();
        }
    }

    fn format_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
        append_space_after_keyword: bool,
    ) {
        let is_prev_pipe = self.is_prev_pipe();
        let indent_adjustment = if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        };

        if node.has_new_line() {
            self.append_indent(indent_level);
        }
        self.output.push_str(&node.to_string());

        if append_space_after_keyword {
            self.append_space();
        }

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
            self.format_node(
                Arc::clone(child),
                if child.has_new_line() {
                    block_indent_level + 1
                } else {
                    block_indent_level
                } + indent_adjustment,
            );
        });

        let mut expr_nodes = node.children.iter().skip(expr_index).peekable();
        let colon_node = expr_nodes.next().unwrap();

        self.format_node(Arc::clone(colon_node), block_indent_level + 1);

        if !expr_nodes.peek().unwrap().has_new_line() {
            self.append_space();
        }

        let block_indent_level = if is_prev_pipe {
            block_indent_level + 2
        } else {
            block_indent_level + 1
        } + indent_adjustment;

        expr_nodes.for_each(|child| {
            self.format_node(Arc::clone(child), block_indent_level);
        });
    }

    fn format_let(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        let indent_level = if self.is_last_line_pipe() {
            block_indent_level
        } else {
            indent_level
        };

        node.children.iter().for_each(|child| {
            let indent_level = if child.has_new_line() {
                indent_level + 1
            } else {
                indent_level
            };

            self.format_node(Arc::clone(child), indent_level);
        });
    }

    fn format_call(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        let current_line_indent = if indent_level == 0 {
            self.current_line_indent()
        } else {
            indent_level
        };

        node.children.iter().for_each(|child| {
            self.format_node(
                Arc::clone(child),
                if child.has_new_line() {
                    current_line_indent + 1
                } else {
                    current_line_indent
                },
            );
        });
    }

    fn format_if(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        let is_prev_pipe = self.is_prev_pipe();
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        let indent_level = if self.is_last_line_pipe() {
            block_indent_level
        } else {
            indent_level
        };

        let indent_adjustment = if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        };

        if let [l_param, cond, r_param, then_colon, then_expr, rest @ ..] = node.children.as_slice()
        {
            self.format_node(Arc::clone(l_param), 0);
            self.format_node(Arc::clone(cond), 0);
            self.format_node(Arc::clone(r_param), 0);
            self.format_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            let block_indent_level = if is_prev_pipe {
                indent_level + 2
            } else {
                indent_level + 1
            } + indent_adjustment;

            self.format_node(Arc::clone(then_expr), block_indent_level);

            let node_indent_level = if is_prev_pipe {
                indent_level + 1
            } else {
                indent_level
            } + indent_adjustment;

            for child in rest {
                self.format_node(Arc::clone(child), node_indent_level);
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
            self.format_node(Arc::clone(l_param), 0);
            self.format_node(Arc::clone(cond), 0);
            self.format_node(Arc::clone(r_param), 0);
            self.format_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.format_node(Arc::clone(then_expr), indent_level + 1);
        }
    }

    fn format_else(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        if let [then_colon, then_expr] = node.children.as_slice() {
            self.format_node(Arc::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.format_node(Arc::clone(then_expr), indent_level + 1);
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
                    if node.has_new_line() {
                        self.append_indent(indent_level);
                    }

                    if !self.output.is_empty()
                        && !self.output.ends_with('\n')
                        && !self.output.ends_with(' ')
                    {
                        self.append_space();
                    }

                    self.output.push_str(&comment.to_string());
                }
                mq_lang::CstTrivia::NewLine => {
                    self.output.push('\n');
                }
                _ => {}
            }
        }
    }

    fn append_indent(&mut self, level: usize) {
        self.output
            .push_str(&" ".repeat(level * self.config.indent_width));
    }

    fn append_env(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Env,
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.output.push_str(&token.to_string());
        }
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
                mq_lang::TokenKind::StringLiteral(s) => {
                    let escaped = Self::escape_string(s);
                    self.output.push_str(&format!(r#""{}""#, escaped));
                }
                mq_lang::TokenKind::NumberLiteral(n) => self.output.push_str(&n.to_string()),
                mq_lang::TokenKind::BoolLiteral(b) => self.output.push_str(&b.to_string()),
                mq_lang::TokenKind::None => self.output.push_str(&token.to_string()),
                _ => unreachable!(),
            }
        }
    }

    fn append_interpolated_string(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::InterpolatedString,
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.output.push_str(&format!(
                "s\"{}\"",
                token
                    .to_string()
                    .replace("\"", "\\\"")
                    .replace("\t", "\\t")
                    .replace("\r", "\\r")
            ))
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
                self.format_node(Arc::clone(child), indent_level);
            });
        }
    }

    fn format_keyword(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            token: Some(token), ..
        } = &**node
        {
            match token.kind {
                mq_lang::TokenKind::End => {
                    if node.has_new_line() {
                        let indent_level = indent_level.saturating_sub(1);
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.output.push_str(&token.to_string());
                    } else {
                        if !self.output.ends_with(' ') {
                            self.append_space();
                        }
                        self.output.push_str(&token.to_string());
                    }
                }
                _ => {
                    self.append_indent(indent_level);
                    self.output.push_str(&token.to_string());
                }
            }
        }
    }

    fn append_token(&mut self, node: &Arc<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Token,
            token: Some(token),
            ..
        } = &**node
        {
            match token.kind {
                mq_lang::TokenKind::Comma => {
                    if node.has_new_line() {
                        self.append_leading_trivia(node, indent_level);
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
                mq_lang::TokenKind::RParen => {
                    if node.has_new_line() {
                        let indent_level = indent_level.saturating_sub(1);
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.output.push_str(&token.to_string());
                    } else {
                        self.output.push_str(&token.to_string());
                    }
                }
                _ => self.output.push_str(&token.to_string()),
            }
        }
    }

    #[inline(always)]
    fn append_space(&mut self) {
        self.output.push(' ');
    }

    #[inline(always)]
    fn append_newline(&mut self) {
        self.output.push('\n');
    }

    #[inline(always)]
    fn is_prev_pipe(&self) -> bool {
        self.output.ends_with("| ")
    }

    #[inline(always)]
    pub fn current_line_indent(&self) -> usize {
        if let Some(last_line) = self.output.lines().last() {
            last_line.chars().take_while(|c| *c == ' ').count() / self.config.indent_width
        } else {
            0
        }
    }

    #[inline(always)]
    pub fn is_last_line_pipe(&self) -> bool {
        let output = self.output.trim_end_matches('\n');
        let lines = output.lines();

        if let Some(last_line) = lines.last() {
            last_line.trim_start().starts_with('|')
        } else {
            false
        }
    }

    #[inline(always)]
    fn is_let_line(&self) -> bool {
        if let Some(last_line) = self.output.lines().last() {
            (!last_line.starts_with("let ") && last_line.trim().starts_with("let "))
                || last_line.trim().replace(" ", "").starts_with("|let")
        } else {
            false
        }
    }

    /// Escapes control characters in a string, preserving existing valid escape sequences
    fn escape_string(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 2);

        for ch in s.chars() {
            match ch {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\t' => result.push_str("\\t"),
                '\r' => result.push_str("\\r"),
                c if c.is_control() => {
                    let code = c as u32;
                    if code <= 0xFF {
                        result.push_str(&format!("\\x{:02x}", code));
                    } else {
                        result.push_str(&format!("\\u{{{:04x}}}", code));
                    }
                }
                c => result.push(c),
            }
        }

        result
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
    #[case("test()|test2()", "test() | test2()")]
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
        "foreach(x,array(1,2,3)):add(x,1);|add(1,2);",
        "foreach (x, array(1, 2, 3)): add(x, 1); | add(1, 2);"
    )]
    #[case::foreach_one_line(".[]|upcase()", ".[] | upcase()")]
    #[case::while_multiline(
        "while(condition()):
        process();",
        "while (condition()):
  process();"
    )]
    #[case::while_oneline("while(condition()): process();", "while (condition()): process();")]
    #[case::while_with_pipe(
        "while(check_condition()):
        data
        | process()
        | output();",
        "while (check_condition()):
  data
  | process()
  | output();"
    )]
    #[case::until_multiline(
        "until(finished()):
        continue_process();",
        "until (finished()):
  continue_process();"
    )]
    #[case::until_oneline(
        "until(finished()): continue_process();",
        "until (finished()): continue_process();"
    )]
    #[case::def(
        r##".h
| let link = to_link(add("#", to_text(self)), to_text(self), "");
| if (eq(to_md_name(), "h1")):
to_md_list(link, 1)
elif (eq(to_md_name(),"h2")):
to_md_list(link, 2)
elif (eq(to_md_name(), "h3")):
to_md_list(link, 3)
elif (eq(to_md_name(), "h4")):
to_md_list(link, 4)
elif (eq(to_md_name(), "h5")):
to_md_list(link, 5)
else:
None"##,
        r##".h
| let link = to_link(add("#", to_text(self)), to_text(self), "");
| if (eq(to_md_name(), "h1")):
    to_md_list(link, 1)
  elif (eq(to_md_name(), "h2")):
    to_md_list(link, 2)
  elif (eq(to_md_name(), "h3")):
    to_md_list(link, 3)
  elif (eq(to_md_name(), "h4")):
    to_md_list(link, 4)
  elif (eq(to_md_name(), "h5")):
    to_md_list(link, 5)
  else:
    None"##
    )]
    #[case::def(
        r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
  let first_char = upcase(first(word))
  | let rest_str = downcase(slice(word, 1, len(word)))
  | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()"#,
        r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()"#
    )]
    #[case::def(
        r#"def snake_to_camel(x): let words = split(x, "_") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join("");| snake_to_camel()"#,
        r#"def snake_to_camel(x): let words = split(x, "_") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join(""); | snake_to_camel()"#
    )]
    #[case::let_(r#"let test = "test""#, r#"let test = "test""#)]
    #[case::call(
        r#"test(
"test")"#,
        r#"test(
  "test")"#
    )]
    #[case::call(
        r#"test(
"test"
  )"#,
        r#"test(
  "test"
)"#
    )]
    #[case::call(
        r#"test(
"test"
,"test"
,true
  )"#,
        r#"test(
  "test"
  ,"test"
  ,true
)"#
    )]
    #[case::interpolated_string(
        r#"test(
s"test${val1}"
  )"#,
        r#"test(
  s"test${val1}"
)"#
    )]
    #[case::include("include  \"test.mq\"", "include \"test.mq\"")]
    #[case::nodes("nodes|nodes", "nodes | nodes")]
    #[case::fn_("fn(): program;", "fn(): program;")]
    #[case::fn_multiline(
        "fn(arg1,arg2):
        program;",
        "fn(arg1, arg2):
  program;"
    )]
    #[case::fn_args("map( fn():program;)", "map(fn(): program;)")]
    #[case::array_empty("[]", "[]")]
    #[case::array_single_element("[1]", "[1]")]
    #[case::array_multiple_elements("[1,2,3]", "[1, 2, 3]")]
    #[case::array_mixed_types("[1,\"test\",true]", "[1, \"test\", true]")]
    #[case::array_nested("[[1,2],[3,4]]", "[[1, 2], [3, 4]]")]
    #[case::array_with_spaces("[ 1 , 2 , 3 ]", "[1, 2, 3]")]
    #[case::array_multiline(
        r#"[
    1,
    2,
    3
    ]"#,
        "[\n  1,\n  2,\n  3\n]"
    )]
    #[case::let_with_array(r#"let arr = [1, 2, 3]"#, r#"let arr = [1, 2, 3]"#)]
    #[case::let_with_array_multiline(
        r#"let arr = [
1,
2,
3
]"#,
        "let arr = [\n  1,\n  2,\n  3\n]"
    )]
    #[case::dict_empty("{}", "{}")]
    #[case::def_with_let_and_array(
        r#"def foo():
  let arr = [
  1,
  2,
  3];
"#,
        r#"def foo():
  let arr = [
    1,
    2,
    3
  ];
"#
    )]
    #[case::dict_single_pair("{\"key\": \"value\"}", "{\"key\": \"value\"}")]
    #[case::dict_multiple_pairs(
        "{\"key1\": \"value1\", \"key2\": \"value2\"}",
        "{\"key1\": \"value1\", \"key2\": \"value2\"}"
    )]
    #[case::dict_with_spaces("{ \"key\" : \"value\" }", "{\"key\": \"value\"}")]
    #[case::dict_mixed_types(
        "{\"str\": \"value\", \"num\": 42, \"bool\": true}",
        "{\"str\": \"value\", \"num\": 42, \"bool\": true}"
    )]
    #[case::dict_nested(
        "{\"outer\": {\"inner\": \"value\"}}",
        "{\"outer\": {\"inner\": \"value\"}}"
    )]
    #[case::dict_multiline(
        r#"{
"key1": "value1",
"key2": "value2"
}"#,
        "{\n  \"key1\": \"value1\",\n  \"key2\": \"value2\"\n}"
    )]
    #[case::dict_multiline_mixed(
        r#"{
"str": "value",
"num": 42,
"bool": true
}"#,
        "{\n  \"str\": \"value\",\n  \"num\": 42,\n  \"bool\": true\n}"
    )]
    #[case::equal_operator("let x = 1 == 2", "let x = 1 == 2")]
    #[case::not_equal_operator("let y = 3 != 4", "let y = 3 != 4")]
    #[case::string_with_newline(r#""line1\nline2""#, r#""line1\nline2""#)]
    #[case::plus_operator("let x = 1 + 2", "let x = 1 + 2")]
    #[case::let_newline_after_equal(
        r#"let x =
"test""#,
        r#"let x =
  "test""#
    )]
    #[case::let_with_if_multiline(
        r#"let x = if(test):
test
else:
test2"#,
        r#"let x = if (test):
  test
else:
  test2"#
    )]
    #[case::let_with_while_multiline(
        r#"let x = while(condition()):
process();"#,
        r#"let x = while (condition()):
  process();"#
    )]
    #[case::less_than_operator("let x = 1 < 2", "let x = 1 < 2")]
    #[case::less_than_equal_operator("let x = 1 <= 2", "let x = 1 <= 2")]
    #[case::greater_than_operator("let x = 2 > 1", "let x = 2 > 1")]
    #[case::greater_than_equal_operator("let x = 2 >= 1", "let x = 2 >= 1")]
    #[case::range_operator("1..1", "1..1")]
    #[case::range_operator_with_spaces("1 .. 1", "1..1")]
    #[case::range_operator_with_variables("x..y", "x..y")]
    #[case::range_operator_with_string(r#""1" .. "2""#, r#""1".."2""#)]
    #[case::selector_attr(".code.lang", ".code.lang")]
    #[case::env("let ENV = $env", "let ENV = $env")]
    #[case::mul("1 * 1", "1 * 1")]
    #[case::mul("1 / 1", "1 / 1")]
    #[case::and("true && false", "true && false")]
    #[case::or("true || false", "true || false")]
    #[case::binary_op_multiline_or(
        r#"1
|| 2
|| 3"#,
        "1
 || 2
 || 3"
    )]
    #[case::binary_op_multiline(
        r#"let v = 1
|| 2
|| 3"#,
        "let v = 1
   || 2
   || 3"
    )]
    #[case::def_contains(
        r#"def contains(haystack, needle):
if (is_dict(haystack)):
  not(is_none(get(haystack, needle)))
else:
  index(haystack, needle) != -1;
"#,
        r#"def contains(haystack, needle):
  if (is_dict(haystack)):
    not(is_none(get(haystack, needle)))
  else:
    index(haystack, needle) != -1;
"#
    )]
    #[case::escape_sequence_clear_screen(r#""\x1b[2J\x1b[H""#, r#""\x1b[2J\x1b[H""#)]
    #[case::control_character_bell(r#""\x07""#, r#""\x07""#)]
    #[case::control_character_backspace(r#""\x08""#, r#""\x08""#)]
    #[case::control_character_vertical_tab(r#""\x0b""#, r#""\x0b""#)]
    #[case::control_character_form_feed(r#""\x0c""#, r#""\x0c""#)]
    #[case::control_character_escape(r#""\x1b""#, r#""\x1b""#)]
    #[case::control_character_delete(r#""\x7f""#, r#""\x7f""#)]
    #[case::not_operator("!true", "!true")]
    #[case::let_with_if_multiline_in_while(
        r#"while(condition()):
  let x = 1
  | let y = if(test):
test
else:
test2
"#,
        r#"while (condition()):
  let x = 1
  | let y = if (test):
      test
    else:
      test2
"#
    )]
    #[case::let_with_until_multiline(
        r#"let x = until(condition()):
process();"#,
        r#"let x = until (condition()):
  process();"#
    )]
    #[case::let_with_until_multiline2(
        r#""test"
| let x = until(condition()):
process();"#,
        r#""test"
| let x = until (condition()):
  process();"#
    )]
    #[case::let_with_while_multiline(
        r#"let x = while(condition()):
process();"#,
        r#"let x = while (condition()):
  process();"#
    )]
    #[case::let_with_while_multiline2(
        r#""test"
| let x = while(condition()):
process();"#,
        r#""test"
| let x = while (condition()):
  process();"#
    )]
    #[case::array_index_access("let arr = [1, 2, 3]\n|arr[1]", "let arr = [1, 2, 3]\n| arr[1]")]
    #[case::array_index_access_inline("arr[0]", "arr[0]")]
    #[case::dict_index_access(
        "let d = {\"key\": \"value\"}\n|d[\"key\"]",
        "let d = {\"key\": \"value\"}\n| d[\"key\"]"
    )]
    #[case::dict_index_access_inline("d[\"key\"]", "d[\"key\"]")]
    #[case::comment_first_line("# comment\nlet x = 1", "# comment\nlet x = 1")]
    #[case::comment_inline("let x = 1 # inline comment", "let x = 1 # inline comment")]
    #[case::comment_multiline(
        "let x = 1\n# multiline comment\nlet y = 2",
        "let x = 1\n# multiline comment\nlet y = 2"
    )]
    #[case::comment_after_expr_multiline(
        "if(test):\n  test # comment\nelse:\n  test2 # comment2",
        "if (test):\n  test # comment\nelse:\n  test2 # comment2"
    )]
    #[case::comment_after_expr_inline(
        "if(test): test # comment else: test2 # comment2",
        "if (test): test # comment else: test2 # comment2"
    )]
    #[case::fn_as_call_arg_single_line("map(fn(): program;)", "map(fn(): program;)")]
    #[case::fn_as_call_arg_multi_line(
        "map(\n  fn(arg):\n    process(arg);\n)",
        "map(\n  fn(arg):\n    process(arg);\n)"
    )]
    #[case::fn_as_call_arg_with_other_args(
        "map(fn(): program;, 1, \"test\")",
        "map(fn(): program;, 1, \"test\")"
    )]
    #[case::nested_fn_as_call_arg("outer(map(fn(): inner();))", "outer(map(fn(): inner();))")]
    #[case::fn_as_call_arg_with_multiline_args(
        "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)",
        "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)"
    )]
    #[case::group_simple("(1)", "(1)")]
    #[case::group_with_expr("(1 + 2)", "(1 + 2)")]
    #[case::group_with_nested_group("((1 + 2) * 3)", "((1 + 2) * 3)")]
    #[case::group_with_multiple_ops("(1 + 2 * 3)", "(1 + 2 * 3)")]
    #[case::group_with_array("(array(1, 2))", "(array(1, 2))")]
    #[case::group_with_dict("({\"key\": \"value\"})", "({\"key\": \"value\"})")]
    #[case::group_with_comment("(1 + 2) # group comment", "(1 + 2) # group comment")]
    #[case::group_with_call("(test(1, 2))", "(test(1, 2))")]
    #[case::group_with_if("(if(test): test else: test2)", "(if (test): test else: test2)")]
    #[case::group_with_let("(let x = 1)", "(let x = 1)")]
    #[case::fn_end("fn(): test end", "fn(): test end")]
    #[case::negate_operator("-v", "-v")]
    fn test_format(#[case] code: &str, #[case] expected: &str) {
        let result = Formatter::new(None).format(code);
        assert_eq!(result.unwrap(), expected);
    }
}
