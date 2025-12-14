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
    indent_cache: Vec<String>,
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
            indent_cache: Vec::new(),
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

        self.format_with_cst(&nodes)
    }

    pub fn format_with_cst(
        &mut self,
        nodes: &Vec<mq_lang::Shared<mq_lang::CstNode>>,
    ) -> Result<String, mq_lang::CstErrorReporter> {
        for node in nodes {
            self.format_node(mq_lang::Shared::clone(node), 0);
        }

        if !self.output.contains('\n') {
            return Ok(self.output.trim_end().to_string());
        }

        let mut result = String::with_capacity(self.output.len());
        for line in self.output.lines() {
            result.push_str(line.trim_end());
            result.push('\n');
        }

        Ok(result)
    }

    fn format_node(&mut self, node: mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let has_leading_new_line = node.has_new_line();
        let indent_level_consider_new_line = if has_leading_new_line { indent_level } else { 0 };

        if !matches!(
            node.kind,
            // For CallDynamic, all nodes are output again, so do not output a newline here.
            mq_lang::CstNodeKind::Token
                | mq_lang::CstNodeKind::BinaryOp(_)
                | mq_lang::CstNodeKind::End
                | mq_lang::CstNodeKind::CallDynamic
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
            mq_lang::CstNodeKind::BinaryOp(_) | mq_lang::CstNodeKind::Assign => {
                self.format_binary_op(&node, indent_level);
            }
            mq_lang::CstNodeKind::UnaryOp(_) => {
                self.format_unary_op(&node, indent_level);
            }
            mq_lang::CstNodeKind::Group => {
                self.format_group(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Block => {
                self.format_block(&node, indent_level_consider_new_line, indent_level);
            }
            mq_lang::CstNodeKind::Call => self.format_call(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::CallDynamic => self.format_call_dynamic(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Def
            | mq_lang::CstNodeKind::Foreach
            | mq_lang::CstNodeKind::While
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
            mq_lang::CstNodeKind::If => self.format_if(&node, indent_level_consider_new_line, indent_level),
            mq_lang::CstNodeKind::Include => self.format_include(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Import => self.format_import(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Module => self.format_module(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::QualifiedAccess => {
                self.format_qualified_access(&node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::InterpolatedString => {
                self.append_interpolated_string(&node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Let | mq_lang::CstNodeKind::Var => {
                self.format_var_decl(&node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Literal => self.append_literal(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Env => self.append_env(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Self_
            | mq_lang::CstNodeKind::Break
            | mq_lang::CstNodeKind::Continue => self.format_keyword(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Selector => self.format_selector(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Try => self.format_try(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Catch => self.format_catch(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Match => self.format_match(&node, indent_level_consider_new_line, indent_level),
            mq_lang::CstNodeKind::MatchArm => self.format_match_arm(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Pattern => self.format_pattern(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Token => self.append_token(&node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::DictEntry => self.format_dict_entry(&node, indent_level_consider_new_line),
        }
    }

    fn format_include(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        node.children.iter().for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), indent_level);
        });
    }

    fn format_import(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        for (i, child) in node.children.iter().enumerate() {
            if i > 0 {
                self.append_space();
            }
            self.format_node(mq_lang::Shared::clone(child), indent_level);
        }
    }

    fn format_module(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        node.children.iter().for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), indent_level + 1);
        });
    }

    fn format_qualified_access(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        // Output module name
        self.output.push_str(&node.to_string());

        // Output children (::, identifier, optional args)
        node.children.iter().for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), 0);
        });
    }

    fn format_array(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let len = node.children.len();

        if len == 0 {
            return;
        }

        let indent_adjustment = if self.is_let_line() || self.is_last_line_pipe() {
            self.current_line_indent()
        } else if indent_level == 0 {
            // If indent_level is 0, it means the array is on the same line (no newline)
            // Use the current line indent to calculate the base indent for children
            self.current_line_indent()
        } else {
            0
        };

        let is_multiline = node.children[1].has_new_line();

        for child in &node.children[..len.saturating_sub(1)] {
            self.format_node(mq_lang::Shared::clone(child), indent_level + indent_adjustment + 1);
        }

        if let Some(last) = node.children.last() {
            if is_multiline {
                self.append_newline();
                self.append_indent(indent_level + indent_adjustment);
            }

            self.format_node(mq_lang::Shared::clone(last), indent_level + indent_adjustment);
        }
    }

    fn format_group(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        node.children.iter().for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), indent_level);
        });
    }

    fn format_dict_entry(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);

        let key = node.children.first();
        let colon = node.children.get(1);
        let value = node.children.get(2);

        if let (Some(key), Some(colon), Some(value)) = (key, colon, value) {
            self.format_node(mq_lang::Shared::clone(key), indent_level);
            self.format_node(mq_lang::Shared::clone(colon), 0);
            self.append_space();
            self.format_node(mq_lang::Shared::clone(value), indent_level);
        }
    }

    fn format_dict(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let len = node.children.len();
        let indent_adjustment = if self.is_let_line() || self.is_last_line_pipe() {
            self.current_line_indent()
        } else if indent_level == 0 {
            // If indent_level is 0, it means the dict is on the same line (no newline)
            // Use the current line indent to calculate the base indent for children
            self.current_line_indent()
        } else {
            0
        };

        for child in &node.children[..len.saturating_sub(1)] {
            self.format_node(mq_lang::Shared::clone(child), indent_level + indent_adjustment + 1);
        }

        if let Some(last) = node.children.last() {
            if last.has_new_line() {
                self.append_newline();
                self.append_indent(indent_level + indent_adjustment);
            }

            self.format_node(mq_lang::Shared::clone(last), indent_level + indent_adjustment);
        }
    }

    fn format_binary_op(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, block_indent_level: usize) {
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
                        self.output.push_str(&token.to_string());
                    }
                    mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::BinaryOp(_),
                        token: Some(token),
                        ..
                    }
                    | mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::Assign,
                        token: Some(token),
                        ..
                    } => {
                        self.append_leading_trivia(node, block_indent_level);

                        if node.has_new_line() {
                            self.append_indent(block_indent_level);
                        }

                        self.output.push(' ');
                        self.output.push_str(&token.to_string());
                        self.output.push(' ');
                    }
                    _ => unreachable!(),
                }

                self.format_node(right, block_indent_level);
            }
            _ => unreachable!(),
        }
    }

    fn format_unary_op(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
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
                    self.format_node(mq_lang::Shared::clone(&node.children[0]), indent_level);
                }
            }
        } else {
            unreachable!();
        }
    }

    fn format_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
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
            .unwrap_or(0);

        node.children.iter().take(expr_index).for_each(|child| {
            self.format_node(
                mq_lang::Shared::clone(child),
                if child.has_new_line() {
                    block_indent_level + 1
                } else {
                    block_indent_level
                } + indent_adjustment,
            );
        });

        let mut expr_nodes = node.children.iter().skip(expr_index).peekable();
        let colon_node = expr_nodes.next().unwrap();

        self.format_node(mq_lang::Shared::clone(colon_node), block_indent_level + 1);

        if !expr_nodes.peek().unwrap().has_new_line() {
            self.append_space();
        }

        let block_indent_level = if is_prev_pipe {
            block_indent_level + 2
        } else {
            block_indent_level + 1
        } + indent_adjustment;

        expr_nodes.for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), block_indent_level);
        });
    }

    fn format_block(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
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
        self.append_space();

        let expr_nodes = node.children.iter().peekable();
        let block_indent_level = if is_prev_pipe {
            block_indent_level + 2
        } else {
            block_indent_level + 1
        } + indent_adjustment;

        expr_nodes.for_each(|child| {
            self.format_node(mq_lang::Shared::clone(child), block_indent_level);
        });
    }

    fn format_var_decl(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
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

            self.format_node(mq_lang::Shared::clone(child), indent_level);
        });
    }

    fn format_call(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        let current_line_indent = if indent_level == 0 {
            self.current_line_indent()
        } else {
            indent_level
        };

        node.children.iter().for_each(|child| {
            self.format_node(
                mq_lang::Shared::clone(child),
                if child.has_new_line() {
                    current_line_indent + 1
                } else {
                    current_line_indent
                },
            );
        });
    }

    fn format_call_dynamic(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let current_line_indent = if indent_level == 0 {
            self.current_line_indent()
        } else {
            indent_level
        };

        node.children.iter().for_each(|child| {
            self.format_node(
                mq_lang::Shared::clone(child),
                if child.has_new_line() {
                    current_line_indent + 1
                } else {
                    current_line_indent
                },
            );
        });
    }

    fn format_if(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize, block_indent_level: usize) {
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

        if let [l_param, cond, r_param, then_colon, then_expr, rest @ ..] = node.children.as_slice() {
            self.format_node(mq_lang::Shared::clone(l_param), 0);
            self.format_node(mq_lang::Shared::clone(cond), 0);
            self.format_node(mq_lang::Shared::clone(r_param), 0);
            self.format_node(mq_lang::Shared::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            let block_indent_level = if is_prev_pipe {
                indent_level + 2
            } else {
                indent_level + 1
            } + indent_adjustment;

            self.format_node(mq_lang::Shared::clone(then_expr), block_indent_level);

            let node_indent_level = if is_prev_pipe { indent_level + 1 } else { indent_level } + indent_adjustment;

            for child in rest {
                self.format_node(mq_lang::Shared::clone(child), node_indent_level);
            }
        }
    }

    fn format_elif(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
        self.append_space();

        if let [l_param, cond, r_param, then_colon, then_expr] = node.children.as_slice() {
            self.format_node(mq_lang::Shared::clone(l_param), 0);
            self.format_node(mq_lang::Shared::clone(cond), 0);
            self.format_node(mq_lang::Shared::clone(r_param), 0);
            self.format_node(mq_lang::Shared::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.format_node(mq_lang::Shared::clone(then_expr), indent_level + 1);
        }
    }

    fn format_else(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        if let [then_colon, then_expr] = node.children.as_slice() {
            self.format_node(mq_lang::Shared::clone(then_colon), 0);

            if !then_expr.has_new_line() {
                self.append_space();
            }

            self.format_node(mq_lang::Shared::clone(then_expr), indent_level + 1);
        }
    }

    fn format_ident(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());
    }

    fn format_try(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        if let Some(colon) = node.children.first() {
            self.output.push_str(&colon.to_string());
            self.append_space();
        }

        for child in node.children.iter().skip(1) {
            if matches!(child.kind, mq_lang::CstNodeKind::Catch) {
                self.format_node(mq_lang::Shared::clone(child), indent_level);
            } else {
                let child_indent = if child.has_new_line() {
                    indent_level + 1
                } else {
                    indent_level
                };
                self.format_node(mq_lang::Shared::clone(child), child_indent);
            }
        }
    }

    fn format_catch(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.output.push_str(&node.to_string());

        if let Some(colon) = node.children.first() {
            self.output.push_str(&colon.to_string());
            self.append_space();
        }

        for child in node.children.iter().skip(1) {
            let child_indent = if child.has_new_line() {
                indent_level + 1
            } else {
                indent_level
            };
            self.format_node(mq_lang::Shared::clone(child), child_indent);
        }
    }

    fn format_match(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
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

        // Find the colon position
        let colon_pos = node
            .children
            .iter()
            .position(|c| {
                c.token
                    .as_ref()
                    .map(|t| matches!(t.kind, mq_lang::TokenKind::Colon))
                    .unwrap_or(false)
            })
            .unwrap_or(0);

        // Format arguments (lparen, value, rparen)
        for child in node.children.iter().take(colon_pos) {
            self.format_node(mq_lang::Shared::clone(child), 0);
        }

        // Format colon
        if let Some(colon) = node.children.get(colon_pos) {
            self.format_node(mq_lang::Shared::clone(colon), 0);
        }

        // Calculate indent level for match arms (similar to format_if)
        let node_indent_level = if is_prev_pipe {
            indent_level + 2
        } else {
            indent_level + 1
        } + indent_adjustment;

        // Calculate indent level for end keyword
        let end_indent_level = if is_prev_pipe { indent_level + 1 } else { indent_level } + indent_adjustment;

        // Format match arms and end
        let remaining_children: Vec<_> = node.children.iter().skip(colon_pos + 1).collect();

        // Check if this is a multiline match (first match arm has new line)
        let is_multiline = remaining_children
            .iter()
            .any(|child| matches!(child.kind, mq_lang::CstNodeKind::MatchArm) && child.has_new_line());

        for (i, child) in remaining_children.iter().enumerate() {
            // Check if this is the last child and it's an End node
            if i == remaining_children.len() - 1 && matches!(child.kind, mq_lang::CstNodeKind::End) {
                // Add newline before end for multiline match
                if is_multiline {
                    self.append_newline();
                    self.append_indent(end_indent_level);
                    self.output.push_str("end");
                    continue;
                }
            }
            self.format_node(mq_lang::Shared::clone(child), node_indent_level);
        }
    }

    /// Formats a match arm node, handling pipe, pattern, optional guard, colon, and body.
    fn format_match_arm(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let children = &node.children;
        let mut idx = 0;

        // 1. Pipe
        if let Some(pipe) = children.get(idx) {
            if node.has_new_line() {
                self.append_indent(indent_level);
                self.output.push('|');
                self.append_space();
            } else {
                self.format_node(mq_lang::Shared::clone(pipe), 0);
            }
            idx += 1;
        }

        // 2. Pattern
        if let Some(pattern) = children.get(idx) {
            self.format_node(mq_lang::Shared::clone(pattern), 0);
            idx += 1;
        }

        // 3. Optional guard: if <expr>
        if let Some(if_token) = children.get(idx)
            && let Some(token) = if_token.token.as_ref()
            && matches!(token.kind, mq_lang::TokenKind::If)
        {
            self.append_space();
            self.output.push_str("if ");
            idx += 1;

            // Guard expression: all nodes until colon
            while let Some(expr) = children.get(idx) {
                if let Some(t) = expr.token.as_ref()
                    && matches!(t.kind, mq_lang::TokenKind::Colon)
                {
                    break;
                }
                self.format_node(mq_lang::Shared::clone(expr), 0);
                idx += 1;
            }
        }

        // 4. Colon
        if let Some(colon) = children.get(idx)
            && let Some(token) = colon.token.as_ref()
            && matches!(token.kind, mq_lang::TokenKind::Colon)
        {
            self.output.push_str(&token.to_string());
            self.append_space();
            idx += 1;
        }

        // 5. Body
        if let Some(body) = children.get(idx) {
            self.format_node(mq_lang::Shared::clone(body), indent_level + 1);
        }
    }

    fn format_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if indent_level > 0 {
            self.append_indent(indent_level);
        }

        // If pattern has a token, it's a simple pattern (literal, ident, wildcard)
        if let Some(token) = &node.token {
            match &token.kind {
                mq_lang::TokenKind::StringLiteral(s) => {
                    let escaped = Self::escape_string(s);
                    self.output.push('"');
                    self.output.push_str(&escaped);
                    self.output.push('"');
                }
                mq_lang::TokenKind::NumberLiteral(n) => self.output.push_str(&n.to_string()),
                mq_lang::TokenKind::BoolLiteral(b) => self.output.push_str(&b.to_string()),
                mq_lang::TokenKind::None => self.output.push_str(&token.to_string()),
                mq_lang::TokenKind::Ident(name) => self.output.push_str(name),
                _ => {}
            }
        }

        // Format children (for complex patterns like arrays, dicts, type patterns)
        if !node.children.is_empty() {
            // Check if this is a type pattern (starts with colon)
            if let Some(first) = node.children.first()
                && let Some(token) = &first.token
            {
                if matches!(token.kind, mq_lang::TokenKind::Colon) {
                    // Type pattern: :type_name
                    self.output.push_str(&token.to_string());
                    if let Some(second) = node.children.get(1)
                        && let Some(t) = &second.token
                        && let mq_lang::TokenKind::Ident(name) = &t.kind
                    {
                        self.output.push_str(name);
                    }
                    return;
                } else if matches!(token.kind, mq_lang::TokenKind::LBracket) {
                    self.format_array_pattern(node);
                    return;
                } else if matches!(token.kind, mq_lang::TokenKind::LBrace) {
                    self.format_dict_pattern(node);
                    return;
                }
            }

            for child in &node.children {
                self.format_node(mq_lang::Shared::clone(child), indent_level);
            }
        }
    }

    fn format_array_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        for child in &node.children {
            match &child.kind {
                mq_lang::CstNodeKind::Token => {
                    if let Some(token) = &child.token {
                        match &token.kind {
                            mq_lang::TokenKind::Comma => self.output.push_str(", "),
                            _ => self.output.push_str(&token.to_string()),
                        }
                    }
                }
                _ => self.output.push_str(&child.to_string()),
            }

            child.children.iter().for_each(|gc| {
                self.format_node(mq_lang::Shared::clone(gc), 0);
            });
        }
    }

    fn format_dict_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        let mut i = 0;
        let children = &node.children;

        while i < children.len() {
            let child = &children[i];

            if let Some(token) = &child.token {
                match &token.kind {
                    mq_lang::TokenKind::Comma => {
                        self.output.push_str(", ");
                        i += 1;
                        continue;
                    }
                    mq_lang::TokenKind::Ident(name) => {
                        self.output.push_str(name);

                        // Check for colon and pattern after the identifier
                        if let Some(next) = children.get(i + 1)
                            && let Some(next_token) = &next.token
                            && matches!(next_token.kind, mq_lang::TokenKind::Colon)
                        {
                            self.output.push_str(&next_token.to_string());
                            self.output.push(' ');
                            i += 2; // Skip colon

                            // Format the pattern after colon
                            if let Some(pattern_node) = children.get(i)
                                && let Some(pattern_token) = &pattern_node.token
                                && let mq_lang::TokenKind::Ident(pattern_name) = &pattern_token.kind
                            {
                                self.output.push_str(pattern_name);
                            }
                        }
                    }
                    _ => {
                        self.output.push_str(&token.to_string());
                        i += 1;
                        continue;
                    }
                }
            }

            child.children.iter().for_each(|gc| {
                self.format_node(mq_lang::Shared::clone(gc), 0);
            });

            i += 1;
        }
    }

    fn append_leading_trivia(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        for trivia in &node.leading_trivia {
            match trivia {
                mq_lang::CstTrivia::Whitespace(_) => {}
                comment @ mq_lang::CstTrivia::Comment(_) => {
                    if self.is_prev_pipe() {
                        self.append_space();
                    } else if node.has_new_line() && self.output.ends_with('\n') {
                        self.append_indent(indent_level);
                    }

                    if !self.output.is_empty() && !self.output.ends_with('\n') && !self.output.ends_with(' ') {
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
        // Ensure cache has enough entries
        while self.indent_cache.len() <= level {
            let next_level = self.indent_cache.len();
            let indent_str = " ".repeat(next_level * self.config.indent_width);
            self.indent_cache.push(indent_str);
        }

        self.output.push_str(&self.indent_cache[level]);
    }

    fn append_env(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
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

    fn append_literal(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        // Check if this is a symbol literal (has children: colon token + identifier/string)
        if !node.children.is_empty() {
            self.append_symbol(node, indent_level);
            return;
        }

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
                    self.output.push('"');
                    self.output.push_str(&escaped);
                    self.output.push('"');
                }
                mq_lang::TokenKind::NumberLiteral(n) => self.output.push_str(&n.to_string()),
                mq_lang::TokenKind::BoolLiteral(b) => self.output.push_str(&b.to_string()),
                mq_lang::TokenKind::None => self.output.push_str(&token.to_string()),
                other => {
                    eprintln!(
                        "Warning: Unexpected token kind in append_literal: {:?}. Inserting placeholder.",
                        other
                    );
                    self.output.push_str(&other.to_string());
                }
            }
        }
    }

    fn append_symbol(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        // Format symbol as :ident or :"string"
        for child in &node.children {
            if let Some(token) = &child.token {
                match &token.kind {
                    mq_lang::TokenKind::Colon => self.output.push(':'),
                    mq_lang::TokenKind::Ident(s) => self.output.push_str(s),
                    mq_lang::TokenKind::StringLiteral(s) => {
                        let escaped = Self::escape_string(s);
                        self.output.push('"');
                        self.output.push_str(&escaped);
                        self.output.push('"');
                    }
                    _ => {}
                }
            }
        }
    }

    fn append_interpolated_string(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::InterpolatedString,
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.output.push_str("s\"");
            let escaped = token
                .to_string()
                .replace("\\", "\\\\") // Must be first to avoid double-escaping
                .replace("\"", "\\\"")
                .replace("\t", "\\t")
                .replace("\r", "\\r");
            self.output.push_str(&escaped);
            self.output.push('"');
        }
    }

    fn format_selector(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
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
                self.format_node(mq_lang::Shared::clone(child), indent_level);
            });
        }
    }

    fn format_keyword(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode { token: Some(token), .. } = &**node {
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

    fn append_token(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
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
                        self.output.push_str(&token.to_string());
                    } else {
                        self.output.push_str(&token.to_string());
                        self.output.push(' ');
                    }
                }
                mq_lang::TokenKind::Colon => self.output.push_str(&token.to_string()),
                mq_lang::TokenKind::Equal => {
                    self.output.push(' ');
                    self.output.push_str(&token.to_string());
                    self.output.push(' ');
                }
                mq_lang::TokenKind::Pipe => {
                    if node.has_new_line() {
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.output.push_str(&token.to_string());
                        self.output.push(' ');
                    } else {
                        self.output.push(' ');
                        self.output.push_str(&token.to_string());
                        self.output.push(' ');
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
        // Find the last newline position
        let start = self.output.rfind('\n').map_or(0, |pos| pos + 1);
        let last_line = &self.output[start..];
        last_line.chars().take_while(|c| *c == ' ').count() / self.config.indent_width
    }

    #[inline(always)]
    pub fn is_last_line_pipe(&self) -> bool {
        let output = self.output.trim_end_matches('\n');
        // Find the last newline position
        let start = output.rfind('\n').map_or(0, |pos| pos + 1);
        let last_line = &output[start..];
        last_line.trim_start().starts_with('|')
    }

    #[inline(always)]
    fn is_let_line(&self) -> bool {
        // Find the last newline position
        let start = self.output.rfind('\n').map_or(0, |pos| pos + 1);

        if start < self.output.len() {
            let last_line = &self.output[start..];
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
    #[case::empty("", "")]
    #[case::if_(
        "if(test):
        test
        else:
        test2",
        "if (test):
  test
else:
  test2
"
    )]
    #[case::def_(
        "def name(test):
        test | test2;",
        "def name(test):
  test | test2;
"
    )]
    #[case::foreach_(
        "foreach(x,array(1, 2, 3)):
        add(x, 1);",
        "foreach (x, array(1, 2, 3)):
  add(x, 1);
"
    )]
    #[case::def_(
        "def test():
        test1
        |test2
        |test3;",
        "def test():
  test1
  | test2
  | test3;
"
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
  test3
"
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
  test3
"
    )]
    #[case::if_(
        "if(test):
        test
        else:
        test2",
        "if (test):
  test
else:
  test2
"
    )]
    #[case::if_else(
        "if(test):
        test
        else: do
        test2
        end",
        "if (test):
  test
else: do
    test2
  end
"
    )]
    #[case::one_line("if(test): test else: test2", "if (test): test else: test2")]
    #[case::one_line(
        "if(test): test elif(test2): test2 else: test3",
        "if (test): test elif (test2): test2 else: test3"
    )]
    #[case::foreach_one_line("foreach(x,array(1,2,3)):add(x,1);", "foreach (x, array(1, 2, 3)): add(x, 1);")]
    #[case::foreach_one_line(
        "foreach(x,array(1,2,3)):add(x,1);|add(1,2);",
        "foreach (x, array(1, 2, 3)): add(x, 1); | add(1, 2);"
    )]
    #[case::foreach_one_line(".[]|upcase()", ".[] | upcase()")]
    #[case::while_multiline(
        "while(condition()):
        process();",
        "while (condition()):
  process();
"
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
  | output();
"
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
    None
"##
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
| snake_to_camel()
"#
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
  "test")
"#
    )]
    #[case::call(
        r#"test(
"test"
  )"#,
        r#"test(
  "test"
)
"#
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
)
"#
    )]
    #[case::interpolated_string(
        r#"test(
s"test${val1}"
  )"#,
        r#"test(
  s"test${val1}"
)
"#
    )]
    #[case::include("include  \"test.mq\"", "include \"test.mq\"")]
    #[case::nodes("nodes|nodes", "nodes | nodes")]
    #[case::fn_("fn(): program;", "fn(): program;")]
    #[case::fn_multiline(
        "fn(arg1,arg2):
        program;",
        "fn(arg1, arg2):
  program;
"
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
        "[\n  1,\n  2,\n  3\n]\n"
    )]
    #[case::let_with_array(r#"let arr = [1, 2, 3]"#, r#"let arr = [1, 2, 3]"#)]
    #[case::let_with_array_multiline(
        r#"let arr = [
1,
2,
3
]"#,
        "let arr = [\n  1,\n  2,\n  3\n]\n"
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
    #[case::dict_nested("{\"outer\": {\"inner\": \"value\"}}", "{\"outer\": {\"inner\": \"value\"}}")]
    #[case::dict_multiline(
        r#"{
"key1": "value1",
"key2": "value2"
}"#,
        "{\n  \"key1\": \"value1\",\n  \"key2\": \"value2\"\n}\n"
    )]
    #[case::dict_multiline_mixed(
        r#"{
"str": "value",
"num": 42,
"bool": true
}"#,
        "{\n  \"str\": \"value\",\n  \"num\": 42,\n  \"bool\": true\n}\n"
    )]
    #[case::equal_operator("let x = 1 == 2", "let x = 1 == 2")]
    #[case::not_equal_operator("let y = 3 != 4", "let y = 3 != 4")]
    #[case::string_with_newline(r#""line1\nline2""#, r#""line1\nline2""#)]
    #[case::plus_operator("let x = 1 + 2", "let x = 1 + 2")]
    #[case::let_newline_after_equal(
        r#"let x =
"test""#,
        r#"let x =
  "test"
"#
    )]
    #[case::let_with_if_multiline(
        r#"let x = if(test):
test
else:
test2"#,
        r#"let x = if (test):
  test
else:
  test2
"#
    )]
    #[case::let_with_while_multiline(
        r#"let x = while(condition()):
process();"#,
        r#"let x = while (condition()):
  process();
"#
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
 || 3
"
    )]
    #[case::binary_op_multiline(
        r#"let v = 1
|| 2
|| 3"#,
        "let v = 1
   || 2
   || 3
"
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
end
"#,
        r#"while (condition()):
  let x = 1
  | let y = if (test):
      test
    else:
      test2
end
"#
    )]
    #[case::let_with_while_multiline(
        r#"let x = while(condition()):
process();"#,
        r#"let x = while (condition()):
  process();
"#
    )]
    #[case::let_with_while_multiline2(
        r#""test"
| let x = while(condition()):
process();"#,
        r#""test"
| let x = while (condition()):
  process();
"#
    )]
    #[case::array_index_access("let arr = [1, 2, 3]\n|arr[1]", "let arr = [1, 2, 3]\n| arr[1]\n")]
    #[case::array_index_access_inline("arr[0]", "arr[0]")]
    #[case::dict_index_access(
        "let d = {\"key\": \"value\"}\n|d[\"key\"]",
        "let d = {\"key\": \"value\"}\n| d[\"key\"]\n"
    )]
    #[case::dict_index_access_inline("d[\"key\"]", "d[\"key\"]")]
    #[case::comment_first_line("# comment\nlet x = 1", "# comment\nlet x = 1\n")]
    #[case::comment_inline("let x = 1 # inline comment", "let x = 1 # inline comment")]
    #[case::comment_multiline(
        "let x = 1\n# multiline comment\n| let y = 2",
        "let x = 1\n# multiline comment\n| let y = 2\n"
    )]
    #[case::comment_after_expr_multiline(
        "if(test):\n  test # comment\nelse:\n  test2 # comment2",
        "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
    )]
    #[case::comment_after_expr_inline(
        "if(test): test # comment else: test2 # comment2",
        "if (test): test # comment else: test2 # comment2"
    )]
    #[case::fn_as_call_arg_single_line("map(fn(): program;)", "map(fn(): program;)")]
    #[case::fn_as_call_arg_multi_line(
        "map(\n  fn(arg):\n    process(arg);\n)",
        "map(\n  fn(arg):\n    process(arg);\n)\n"
    )]
    #[case::fn_as_call_arg_with_other_args("map(fn(): program;, 1, \"test\")", "map(fn(): program;, 1, \"test\")")]
    #[case::nested_fn_as_call_arg("outer(map(fn(): inner();))", "outer(map(fn(): inner();))")]
    #[case::fn_as_call_arg_with_multiline_args(
        "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)",
        "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)\n"
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
    #[case::try_catch_multiline(
        r#"try:
  process()
catch:
  handle_error()"#,
        "try:
  process()
catch:
  handle_error()
"
    )]
    #[case::try_catch_oneline("try: process() catch: handle_error()", "try: process() catch: handle_error()")]
    #[case::try_catch_with_finally(
        r#"try:
  process()
catch:
  handle_error()
"#,
        "try:
  process()
catch:
  handle_error()
"
    )]
    #[case::coalesce_operator("let x = a?? b", "let x = a ?? b")]
    #[case::coalesce_operator_with_call(
        "let result = get_value() ?? default()",
        "let result = get_value() ?? default()"
    )]
    #[case::coalesce_operator_with_literal("let x = value ?? 42", "let x = value ?? 42")]
    #[case::coalesce_operator_with_string("let s = str ?? \"default\"", "let s = str ?? \"default\"")]
    #[case::coalesce_operator_chain("let x = a ?? b ?? c", "let x = a ?? b ?? c")]
    #[case::coalesce_operator_in_if(
        "if(a ?? b): do_something() else: do_other()",
        "if (a ?? b): do_something() else: do_other()"
    )]
    #[case::coalesce_operator_in_array("[a ?? b, c]", "[a ?? b, c]")]
    #[case::coalesce_operator_in_dict("{\"key\": a ?? b}", "{\"key\": a ?? b}")]
    #[case::coalesce_operator_with_comment("let x = a ?? b # fallback", "let x = a ?? b # fallback")]
    #[case::call_dynamic("v[0](1,2,3)", "v[0](1, 2, 3)")]
    #[case::dict_with_fn_value_multiline(
        r#"{
"key1": fn():
process();
,"key2": "value2"
}"#,
        r#"{
  "key1": fn():
    process();
  ,"key2": "value2"
}
"#
    )]
    #[case::do_block_multiline(
        r#"do
  process1()
  | process2()
end
"#,
        "do
  process1()
  | process2()
end
"
    )]
    #[case::do_block_oneline("do process1() | process2();", "do process1() | process2();")]
    #[case::let_with_do_block_multiline(
        r#"let result = do
  step1()
  | step2();
"#,
        "let result = do
  step1()
  | step2();
"
    )]
    #[case::let_with_do_block_oneline("let result = do step1() | step2();", "let result = do step1() | step2();")]
    #[case::symbol_with_ident(":foo", ":foo")]
    #[case::symbol_with_string(r#":"bar""#, r#":"bar""#)]
    #[case::symbol_with_spaces(":  foo", ":foo")]
    #[case::symbol_in_array("[:foo, :bar]", "[:foo, :bar]")]
    #[case::symbol_in_dict(r#"{:key: "value"}"#, r#"{:key: "value"}"#)]
    #[case::symbol_comparison(":foo == :bar", ":foo == :bar")]
    #[case::match_simple(
        "match(x): | 1: \"one\" | _: \"other\" end",
        "match (x): | 1: \"one\" | _: \"other\" end"
    )]
    #[case::match_multiline(
        r#"match(x):
| 1: "one"
| 2: "two"
| _: "other"
end"#,
        r#"match (x):
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
    )]
    #[case::match_with_guard(
        "match(x): | n if(n > 0): \"positive\" | _: \"non-positive\" end",
        "match (x): | n if (n > 0): \"positive\" | _: \"non-positive\" end"
    )]
    #[case::match_with_array_pattern(
        "match(arr): | [a, b]: add(a, b) | _: 0 end",
        "match (arr): | [a, b]: add(a, b) | _: 0 end"
    )]
    #[case::match_with_array_pattern_with_literal(
        "match(arr): | [1, 2]: add(1, 2) | _: 0 end",
        "match (arr): | [1, 2]: add(1, 2) | _: 0 end"
    )]
    #[case::match_with_array_pattern_with_symbol(
        "match(arr): | [:string, :string]: add(1, 2) | _: 0 end",
        "match (arr): | [:string, :string]: add(1, 2) | _: 0 end"
    )]
    #[case::match_with_dict_pattern(
        "match(obj): | {name: n}: n | _: \"unknown\" end",
        "match (obj): | {name: n}: n | _: \"unknown\" end"
    )]
    #[case::match_nested_in_let(
        r#"let result = match(x):
| 1: "one"
| 2: "two"
| _: "other"
end"#,
        r#"let result = match (x):
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
    )]
    #[case::match_with_type_pattern(
        "match(val): | :string: \"is string\" | :number: \"is number\" | _: \"other\" end",
        "match (val): | :string: \"is string\" | :number: \"is number\" | _: \"other\" end"
    )]
    #[case::match_multiline_in_pipe(
        r#""test"
| match(x):
  | 1: "one"
  | 2: "two"
  end"#,
        r#""test"
| match (x):
    | 1: "one"
    | 2: "two"
  end
"#
    )]
    #[case::dict_nested_multiline_level3(
        r#"{
"level1": {
"level2": {
"level3": "value"
}
}
}"#,
        r#"{
  "level1": {
    "level2": {
      "level3": "value"
    }
  }
}
"#
    )]
    #[case::array_nested_multiline_level3(
        r#"[
[
[
"value"
]
]
]"#,
        r#"[
  [
    [
      "value"
    ]
  ]
]
"#
    )]
    #[case::comment_with_newline("# comment\nlet x = 1", "# comment\nlet x = 1\n")]
    #[case::comment_with_indent(
        "if(test):\n  test # comment\nelse:\n  test2 # comment2",
        "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
    )]
    #[case::comment_inline_with_indent("let x = 1 # inline comment", "let x = 1 # inline comment")]
    #[case::comment_multiline_with_indent(
        "let x = 1\n  # multiline comment\n| let y = 2",
        "let x = 1\n# multiline comment\n| let y = 2\n"
    )]
    #[case::comment_after_expr_multiline_with_indent(
        "if(test):\n  test # comment\nelse:\n    test2 # comment2",
        "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
    )]
    #[case::comment_after_expr_inline_with_indent(
        "if(test): test # comment else:    test2 # comment2",
        "if (test): test # comment else:    test2 # comment2"
    )]
    #[case::interpolated_string_with_escaped_brackets(r#"s"\\[${phrase}\\]\\(""#, r#"s"\\[${phrase}\\]\\(""#)]
    #[case::interpolated_string_with_backslash(r#"s"\\test""#, r#"s"\\test""#)]
    #[case::module_with_body(
        r#"module test:
import "foo.mq"
| def main(): test();
end"#,
        r#"module test:
  import "foo.mq"
  | def main(): test();
end
"#
    )]
    #[case::comment_preserves_indent_after_newline(
        "let x = 1\n    # indented comment after newline\n| let y = 2",
        "let x = 1\n# indented comment after newline\n| let y = 2\n"
    )]
    #[case::comment_preserves_indent_after_newline_deep(
        "if(test):\n  test\n    # deeper indented comment\nelse:\n  test2",
        "if (test):\n  test\n# deeper indented comment\nelse:\n  test2\n"
    )]
    #[case::comment_preserves_indent_after_newline_array(
        "[1,\n    # comment for 2\n  2]",
        "[1,\n  # comment for 2\n  2]\n"
    )]
    #[case::comment_preserves_indent_after_newline_dict(
        "{\n  \"a\": 1,\n    # comment for b\n  \"b\": 2\n}",
        "{\n  \"a\": 1,\n  # comment for b\n  \"b\": 2\n}\n"
    )]
    #[case::match_preserves_indent_after_newline(
        "let v = \nmatch (x):\n|    1: \"one\"\n|    2: \"two\"\n  end",
        "let v =\n  match (x):\n    | 1: \"one\"\n    | 2: \"two\"\n  end\n"
    )]
    #[case::match_with_do_block(
        "let result = match (x):\n| 1: do\n    foo() |\n    bar()\n  end\n| 2: \"two\"\nend",
        "let result = match (x):\n  | 1: do\n      foo() |\n      bar()\n    end\n  | 2: \"two\"\nend\n"
    )]
    #[case::assign("var i=0 | i=i + 1", "var i = 0 | i = i + 1")]
    fn test_format(#[case] code: &str, #[case] expected: &str) {
        let result = Formatter::new(None).format(code);
        assert_eq!(result.unwrap(), expected);
    }
}
