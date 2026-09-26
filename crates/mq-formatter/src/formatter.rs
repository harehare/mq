use std::fmt::Write;

use mq_lang::{CstNode, CstNodeKind};

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

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SortPriority {
    Import = 0,
    Include = 1,
    Let = 2,
    Def = 3,
    Other = 4,
}

#[derive(Clone, Debug)]
pub struct FormatterConfig {
    pub indent_width: usize,
    pub sort_imports: bool,
    pub sort_functions: bool,
    pub sort_fields: bool,
    pub max_width: Option<usize>,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            sort_imports: false,
            sort_functions: false,
            sort_fields: false,
            max_width: None,
        }
    }
}

impl From<&CstNode> for SortPriority {
    fn from(node: &CstNode) -> Self {
        match node.kind {
            CstNodeKind::Import { .. } => SortPriority::Import,
            CstNodeKind::Include { .. } => SortPriority::Include,
            CstNodeKind::Let { .. } | CstNodeKind::Var { .. } => SortPriority::Let,
            CstNodeKind::Def { .. } | CstNodeKind::Fn { .. } => SortPriority::Def,
            _ => SortPriority::Other,
        }
    }
}

pub(crate) fn ident(node: &CstNode) -> Option<String> {
    match node.kind {
        CstNodeKind::Import { .. }
        | CstNodeKind::Include { .. }
        | CstNodeKind::Def { .. }
        | CstNodeKind::Let { .. }
        | CstNodeKind::Var { .. }
            if node.token.is_some() =>
        {
            node.children().next().map(|c| c.to_string())
        }
        _ => None,
    }
}

pub fn needs_pipe(node: &CstNode) -> bool {
    !matches!(node.kind, CstNodeKind::Def { .. } | CstNodeKind::Eof)
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

        let (mut nodes, errors) = mq_lang::parse_recovery(code);

        if errors.has_errors() {
            return Err(errors);
        }

        self.format_with_cst(&mut nodes)
    }

    pub fn format_with_cst(
        &mut self,
        nodes: &mut Vec<mq_lang::Shared<mq_lang::CstNode>>,
    ) -> Result<String, mq_lang::CstErrorReporter> {
        if self.config.sort_imports || self.config.sort_functions || self.config.sort_fields {
            let sorted_nodes = self.sort_nodes(nodes);
            for node in &sorted_nodes {
                self.format_node(node, 0);
            }
        } else {
            for node in nodes {
                self.format_node(node, 0);
            }
        }

        if !self.output.contains('\n') {
            return Ok(self.output.trim_end().to_string());
        }

        let mut result = String::with_capacity(self.output.len());
        for line in self.output.lines() {
            result.push_str(line.trim_end());
            result.push('\n');
        }

        self.output.clear();
        Ok(result)
    }

    #[inline(always)]
    fn should_insert_pipe_before(
        node: &mq_lang::Shared<mq_lang::CstNode>,
        index: usize,
        prev_node: &Option<mq_lang::Shared<mq_lang::CstNode>>,
    ) -> bool {
        (needs_pipe(node) && index != 0) || (!node.is_eof() && prev_node.as_ref().is_some_and(|p| needs_pipe(p)))
    }

    fn sort_nodes(
        &mut self,
        nodes: &mut Vec<mq_lang::Shared<mq_lang::CstNode>>,
    ) -> Vec<mq_lang::Shared<mq_lang::CstNode>> {
        nodes.retain(|node| !node.is_pipe());
        nodes.sort_by(|a, b| {
            let priority_a: SortPriority = (&**a).into();
            let priority_b: SortPriority = (&**b).into();

            if priority_a != priority_b {
                return priority_a.cmp(&priority_b);
            }

            match priority_a {
                SortPriority::Import if self.config.sort_imports => {
                    let ident_a = ident(a);
                    let ident_b = ident(b);
                    ident_a.cmp(&ident_b)
                }
                SortPriority::Let if self.config.sort_fields => {
                    let ident_a = ident(a);
                    let ident_b = ident(b);
                    ident_a.cmp(&ident_b)
                }
                SortPriority::Def if self.config.sort_functions => {
                    let ident_a = ident(a);
                    let ident_b = ident(b);
                    ident_a.cmp(&ident_b)
                }
                _ => std::cmp::Ordering::Equal,
            }
        });

        let mut sorted_nodes = Vec::new();
        let mut prev_node: Option<mq_lang::Shared<mq_lang::CstNode>> = None;

        for (i, node) in nodes.iter_mut().enumerate() {
            if Self::should_insert_pipe_before(node, i, &prev_node) {
                let pipe_node = mq_lang::CstNode::new_pipe(i != 0);
                sorted_nodes.push(mq_lang::Shared::new(pipe_node));
            }

            let node = if i == 0 && node.has_new_line() {
                let mut node = (**node).clone();

                if node.has_new_line() {
                    while matches!(node.leading_trivia.first(), Some(mq_lang::CstTrivia::NewLine)) {
                        node.leading_trivia.remove(0);
                    }
                }

                &mq_lang::Shared::new(node)
            } else if !needs_pipe(node)
                && !node
                    .leading_trivia
                    .first()
                    .is_some_and(|t| matches!(t, mq_lang::CstTrivia::NewLine))
            {
                let mut node = (**node).clone();
                node.leading_trivia.insert(0, mq_lang::CstTrivia::NewLine);
                &mq_lang::Shared::new(node)
            } else {
                node
            };

            sorted_nodes.push(mq_lang::Shared::clone(node));
            prev_node = Some(mq_lang::Shared::clone(node));
        }

        sorted_nodes
    }

    fn format_node(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let has_leading_new_line = node.has_new_line() || (node.is_pipe() && self.should_wrap_pipe());
        let indent_level_consider_new_line = if has_leading_new_line { indent_level } else { 0 };

        if !matches!(
            node.kind,
            // For CallDynamic, all nodes are output again, so do not output a newline here.
            mq_lang::CstNodeKind::Token
                | mq_lang::CstNodeKind::BinaryOp { .. }
                | mq_lang::CstNodeKind::End
                | mq_lang::CstNodeKind::Do
                | mq_lang::CstNodeKind::CallDynamic { .. }
        ) {
            self.append_leading_trivia(node, indent_level_consider_new_line);
        }

        match &node.kind {
            mq_lang::CstNodeKind::Array { .. } => {
                self.format_array(node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Dict { .. } => {
                self.format_dict(node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::BinaryOp { .. } | mq_lang::CstNodeKind::Assign { .. } => {
                self.format_binary_op(node, indent_level);
            }
            mq_lang::CstNodeKind::UnaryOp { .. } => {
                self.format_unary_op(node, indent_level);
            }
            mq_lang::CstNodeKind::Group { .. } => {
                self.format_group(node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Block { .. } => {
                self.format_block(node, indent_level_consider_new_line, indent_level);
            }
            mq_lang::CstNodeKind::Call { .. } => self.format_call(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::CallDynamic { .. } => self.format_call_dynamic(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Def { .. }
            | mq_lang::CstNodeKind::Foreach { .. }
            | mq_lang::CstNodeKind::While { .. }
            | mq_lang::CstNodeKind::Until { .. }
            | mq_lang::CstNodeKind::Loop { .. }
            | mq_lang::CstNodeKind::Fn { .. } => self.format_expr(
                node,
                indent_level_consider_new_line,
                indent_level,
                !matches!(
                    node.kind,
                    mq_lang::CstNodeKind::Fn { .. } | mq_lang::CstNodeKind::Loop { .. }
                ),
            ),
            mq_lang::CstNodeKind::Eof => {}
            mq_lang::CstNodeKind::Elif { .. } => self.format_elif(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Else { .. } => self.format_else(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Ident { .. } => self.format_ident(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::If { .. } => self.format_if(node, indent_level_consider_new_line, indent_level),
            mq_lang::CstNodeKind::Unless { .. } => self.format_if(node, indent_level_consider_new_line, indent_level),
            mq_lang::CstNodeKind::Include { .. } => self.format_include(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Import { .. } => self.format_import(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Module { .. } => self.format_module(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::QualifiedAccess { .. } => {
                self.format_qualified_access(node, indent_level_consider_new_line)
            }
            mq_lang::CstNodeKind::InterpolatedString => {
                self.append_interpolated_string(node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::As { .. } => {
                self.format_as_binding(node, indent_level_consider_new_line);
            }
            mq_lang::CstNodeKind::Let { .. } | mq_lang::CstNodeKind::Var { .. } => {
                self.format_var_decl(node, indent_level_consider_new_line, indent_level)
            }
            mq_lang::CstNodeKind::Literal => self.append_literal(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Symbol { .. } => self.append_symbol(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Param { .. } => self.format_param(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Env => self.append_env(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Self_ { .. }
            | mq_lang::CstNodeKind::Do
            | mq_lang::CstNodeKind::Continue => self.format_keyword(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Break { .. } => self.format_break(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Yield { .. } => self.format_break(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Selector { .. }
            | mq_lang::CstNodeKind::SelectorCall { .. }
            | mq_lang::CstNodeKind::SelfAttr => self.format_selector(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Try { .. } => self.format_try(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Catch { .. } => self.format_catch(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Match { .. } => self.format_match(node, indent_level_consider_new_line, indent_level),
            mq_lang::CstNodeKind::MatchArm { .. } => self.format_match_arm(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::OrPattern { .. } => self.format_or_pattern(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Pattern { .. } => self.format_pattern(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Token => self.append_token(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::DictEntry { .. } => self.format_dict_entry(node, indent_level_consider_new_line),
            mq_lang::CstNodeKind::Spread { .. } => self.format_spread(node, indent_level_consider_new_line),
        }
    }

    fn format_include(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        node.children().for_each(|child| {
            self.format_node(child, indent_level);
        });
    }

    fn format_import(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        for (i, child) in node.children().enumerate() {
            if i > 0 {
                self.append_space();
            }
            self.format_node(child, indent_level);
        }
    }

    fn format_module(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        node.children().for_each(|child| {
            self.format_node(child, indent_level + 1);
        });
    }

    fn format_qualified_access(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        // Output module name
        self.append_display(node);

        // Re-derive from the actual line when inline, since the caller's
        // indent_level may not match where this node was written (mirrors format_call).
        let current_line_indent = if indent_level == 0 {
            self.current_line_indent()
        } else {
            indent_level
        };

        // Output children (::, identifier, optional args)
        node.children().for_each(|child| {
            self.format_node(
                child,
                if child.has_new_line() {
                    current_line_indent + 1
                } else {
                    current_line_indent
                },
            );
        });
    }

    fn format_array(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let all_children = node.all_children();
        let len = all_children.len();

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

        let is_multiline = all_children[1].has_new_line();

        for child in &all_children[..len.saturating_sub(1)] {
            self.format_node(child, indent_level + indent_adjustment + 1);
        }

        if let Some(last) = all_children.last() {
            self.format_closing(last, indent_level + indent_adjustment, is_multiline);
        }
    }

    fn format_group(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        node.children().for_each(|child| {
            self.format_node(child, indent_level);
        });
    }

    fn format_dict_entry(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);

        if let mq_lang::CstNodeKind::DictEntry { key, colon, value } = &node.kind {
            self.format_node(key, indent_level);
            self.format_node(colon, 0);
            self.append_space();
            self.format_node(value, indent_level);
        }
    }

    fn format_dict(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        let all_children = node.all_children();
        let len = all_children.len();
        let indent_adjustment = if self.is_let_line() || self.is_last_line_pipe() {
            self.current_line_indent()
        } else if indent_level == 0 {
            // If indent_level is 0, it means the dict is on the same line (no newline)
            // Use the current line indent to calculate the base indent for children
            self.current_line_indent()
        } else {
            0
        };

        for child in &all_children[..len.saturating_sub(1)] {
            self.format_node(child, indent_level + indent_adjustment + 1);
        }

        if let Some(last) = all_children.last() {
            self.format_closing(last, indent_level + indent_adjustment, last.has_new_line());
        }
    }

    /// Formats a closing `]`/`}` with the comments before it.
    fn format_closing(&mut self, last: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize, is_multiline: bool) {
        let mut has_comment = false;
        let mut after_newline = false;
        for trivia in &last.leading_trivia {
            match trivia {
                mq_lang::CstTrivia::NewLine => after_newline = true,
                comment @ mq_lang::CstTrivia::Comment(_) => {
                    if after_newline || self.output.ends_with('\n') {
                        self.append_newline();
                        self.append_indent(indent_level + 1);
                    } else if !self.output.ends_with(' ') {
                        self.append_space();
                    }
                    self.append_display(comment);
                    has_comment = true;
                }
                _ => {}
            }
        }

        if is_multiline || has_comment {
            self.append_newline();
            self.append_indent(indent_level);
        } else if self.output.ends_with(", ") {
            // Single-line trailing comma: `[1, 2,]`
            self.output.pop();
        }

        self.format_node(last, indent_level);
    }

    fn format_binary_op(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, block_indent_level: usize) {
        match node.binary_op() {
            Some((left, right)) => {
                self.format_node(left, block_indent_level);

                match &**node {
                    mq_lang::CstNode {
                        kind:
                            mq_lang::CstNodeKind::BinaryOp {
                                op: mq_lang::CstBinaryOp::RangeOp,
                                ..
                            },
                        token: Some(token),
                        ..
                    } => {
                        self.append_leading_trivia(node, block_indent_level);

                        if node.has_new_line() {
                            self.append_indent(block_indent_level);
                        }
                        self.append_display(token);
                    }
                    mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::BinaryOp { .. },
                        token: Some(token),
                        ..
                    }
                    | mq_lang::CstNode {
                        kind: mq_lang::CstNodeKind::Assign { .. },
                        token: Some(token),
                        ..
                    } => {
                        self.append_leading_trivia(node, block_indent_level);

                        if node.has_new_line() {
                            self.append_indent(block_indent_level);
                        }

                        self.output.push(' ');
                        self.append_display(token);
                        self.output.push(' ');
                    }
                    _ => unreachable!("Expected BinaryOp or Assign node"),
                }

                let indent_level = if right.has_new_line() {
                    block_indent_level + 1
                } else {
                    block_indent_level
                };

                self.format_node(right, indent_level);
            }
            _ => unreachable!("Expected BinaryOp node with two children"),
        }
    }

    fn format_spread(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Spread { operand },
            token: Some(token),
            ..
        } = &**node
        {
            if node.has_new_line() {
                self.append_indent(indent_level);
            }
            self.append_display(token);
            self.format_node(operand, indent_level);
        } else {
            unreachable!("Expected Spread node");
        }
    }

    fn format_unary_op(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::UnaryOp { op, operand },
            token: Some(token),
            ..
        } = &**node
        {
            if node.has_new_line() {
                self.append_indent(indent_level);
            }
            self.append_display(token);

            match op {
                mq_lang::CstUnaryOp::Not | mq_lang::CstUnaryOp::Negate => {
                    self.format_node(operand, indent_level);
                }
            }
        } else {
            unreachable!("Expected UnaryOp node");
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
        let indent_adjustment = self.calculate_indent_adjustment();

        if node.has_new_line() {
            self.append_indent(indent_level);
        }
        self.append_display(node);

        // Re-derive from the actual line when inline, since the parent's
        // block_indent_level may not match where this node was written.
        let block_indent_level = if indent_level == 0 {
            self.current_line_indent()
        } else {
            block_indent_level
        };

        if append_space_after_keyword {
            self.append_space();
        }

        // Check for 'do' keyword or colon
        let do_index = Self::find_token_position(node, |kind| matches!(kind, mq_lang::TokenKind::Do));
        let colon_index = Self::find_token_position(node, |kind| matches!(kind, mq_lang::TokenKind::Colon));
        // uses_do_syntax is true only when 'do' appears before ':' (or when there's no ':')
        let uses_do_syntax = do_index.is_some_and(|di| colon_index.is_none_or(|ci| di < ci));

        // If there's no colon or do, split before the right parenthesis
        let expr_index = if uses_do_syntax {
            do_index.unwrap()
        } else if let Some(ci) = colon_index {
            ci
        } else if let Some(di) = do_index {
            di
        } else {
            Self::find_token_position(node, |kind| matches!(kind, mq_lang::TokenKind::RParen))
                .map(|index| index + 1)
                .unwrap_or(0)
        };

        node.children().take(expr_index).for_each(|child| {
            self.format_node(
                child,
                if child.has_new_line() {
                    block_indent_level + 1
                } else {
                    block_indent_level
                } + indent_adjustment,
            );
        });

        let mut expr_nodes = node.children().skip(expr_index).peekable();

        // Format colon or do keyword if it exists
        if (colon_index.is_some() || do_index.is_some())
            && let Some(separator_node) = expr_nodes.next()
        {
            if uses_do_syntax {
                // Format 'do' keyword with standardized spacing
                self.format_do_with_spacing(separator_node, &mut expr_nodes);
            } else {
                // Format colon with spacing
                self.format_colon_with_spacing(separator_node, &mut expr_nodes, block_indent_level + 1);
            }
        }

        let block_indent_level = if is_prev_pipe {
            block_indent_level + 2
        } else {
            block_indent_level + 1
        } + indent_adjustment;

        expr_nodes.for_each(|child| {
            self.format_node(child, block_indent_level);
        });
    }

    fn needs_descendant_space(
        prev: &mq_lang::Shared<mq_lang::CstNode>,
        current: &mq_lang::Shared<mq_lang::CstNode>,
    ) -> bool {
        let selector_of = |n: &mq_lang::Shared<mq_lang::CstNode>| {
            n.token.as_ref().and_then(|t| mq_lang::Selector::try_from(&**t).ok())
        };

        match selector_of(current) {
            Some(selector) if selector.is_attribute_selector() => false,
            Some(mq_lang::Selector::Property(_)) => !matches!(selector_of(prev), Some(mq_lang::Selector::Property(_))),
            _ => true,
        }
    }

    fn format_block(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        if matches!(
            node.token.as_ref().map(|t| &t.kind),
            Some(mq_lang::TokenKind::Selector(_))
        ) {
            let all_children = node.all_children();
            all_children.iter().enumerate().for_each(|(i, child)| {
                let is_list_iterator = i > 0
                    && matches!(
                        child.token.as_ref().map(|t| &t.kind),
                        Some(mq_lang::TokenKind::Selector(s)) if s == "."
                    );

                if is_list_iterator {
                    child.children().for_each(|bracket_child| {
                        self.format_node(bracket_child, 0);
                    });
                } else {
                    if i > 0 && Self::needs_descendant_space(&all_children[i - 1], child) && !self.output.ends_with(' ')
                    {
                        self.append_space();
                    }
                    self.format_node(child, if i == 0 { indent_level } else { 0 });
                }
            });
            return;
        }

        let is_prev_pipe = self.is_prev_pipe();
        let indent_adjustment = if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        };

        if node.has_new_line() {
            self.append_indent(indent_level);
        } else if !self.output.is_empty() && !self.output.ends_with(' ') {
            self.append_space();
        }

        self.append_display(node);
        self.append_space();

        let all_children = node.all_children();
        let expr_nodes = all_children.iter().peekable();

        let base_indent = if indent_level > 0 {
            indent_level
        } else {
            block_indent_level
        };

        let child_indent_level = if is_prev_pipe { base_indent + 2 } else { base_indent + 1 } + indent_adjustment;

        expr_nodes.for_each(|child| {
            self.format_node(child, child_indent_level);
        });
    }

    fn format_var_decl(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        let indent_level = if self.is_last_line_pipe() {
            block_indent_level
        } else {
            indent_level
        };

        node.children().for_each(|child| {
            let indent_level = if child.has_new_line() {
                indent_level + 1
            } else {
                indent_level
            };

            self.format_node(child, indent_level);
        });
    }

    fn format_as_binding(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if let Some(expr) = node.children().next() {
            self.format_node(expr, indent_level);
        }
        if !self.output.ends_with(' ') {
            self.append_space();
        }
        self.output.push_str("as");
        self.append_space();
        if let Some(name) = node.children().nth(1) {
            self.format_node(name, indent_level);
        }
    }

    fn format_call(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        let current_line_indent = if indent_level == 0 {
            self.current_line_indent()
        } else {
            indent_level
        };

        node.children().for_each(|child| {
            self.format_node(
                child,
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

        node.children().for_each(|child| {
            self.format_node(
                child,
                if child.has_new_line() {
                    current_line_indent + 1
                } else {
                    current_line_indent
                },
            );
        });
    }

    /// Formats the shared `(cond)`/`:`/then-expr prefix common to `if`/`unless`/`elif`.
    fn format_cond_prefix(
        &mut self,
        args: &[mq_lang::Shared<mq_lang::CstNode>],
        colon: &Option<mq_lang::Shared<mq_lang::CstNode>>,
        then_branch: &mq_lang::Shared<mq_lang::CstNode>,
    ) {
        args.iter().for_each(|child| {
            self.format_node(child, 0);
        });
        if let Some(colon_node) = colon {
            self.format_node(colon_node, 0);
        }
        if !then_branch.has_new_line() && !matches!(then_branch.kind, mq_lang::CstNodeKind::Block { .. }) {
            self.append_space();
        }
    }

    fn format_if(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize, block_indent_level: usize) {
        let is_prev_pipe = self.is_prev_pipe();
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        let indent_level = if self.is_last_line_pipe() {
            block_indent_level
        } else {
            indent_level
        };
        let indent_adjustment = self.calculate_indent_adjustment();

        let then_indent_level = (if is_prev_pipe {
            indent_level + 2
        } else {
            indent_level + 1
        }) + indent_adjustment;
        let node_indent_level = (if is_prev_pipe { indent_level + 1 } else { indent_level }) + indent_adjustment;

        match &node.kind {
            mq_lang::CstNodeKind::If {
                args,
                colon,
                then_branch,
                elifs,
                else_branch,
            } => {
                self.format_cond_prefix(args, colon, then_branch);
                self.format_node(then_branch, then_indent_level);

                for elif in elifs {
                    self.format_node(elif, node_indent_level);
                }
                if let Some(else_branch) = else_branch {
                    self.format_node(else_branch, node_indent_level);
                }
            }
            mq_lang::CstNodeKind::Unless {
                args,
                colon,
                then_branch,
            } => {
                self.format_cond_prefix(args, colon, then_branch);
                self.format_node(then_branch, then_indent_level);
            }
            _ => unreachable!("Expected If or Unless node"),
        }
    }

    fn format_elif(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        let mq_lang::CstNodeKind::Elif {
            args,
            colon,
            then_branch,
        } = &node.kind
        else {
            unreachable!("Expected Elif node");
        };
        self.format_cond_prefix(args, colon, then_branch);
        self.format_node(then_branch, indent_level + 1);
    }

    fn format_else(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }
        self.append_indent(indent_level);
        self.append_display(node);

        let mq_lang::CstNodeKind::Else { colon, then_branch } = &node.kind else {
            unreachable!("Expected Else node");
        };
        self.format_cond_prefix(&[], colon, then_branch);
        self.format_node(then_branch, indent_level + 1);
    }

    fn format_ident(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        if let mq_lang::CstNodeKind::Ident { attr: Some(attr) } = &node.kind {
            // Format the attribute selector directly to avoid a newline from its leading trivia.
            self.format_selector(attr, 0);
        }
    }

    fn format_param(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);

        let mq_lang::CstNodeKind::Param {
            asterisk,
            eq_token,
            default,
        } = &node.kind
        else {
            unreachable!("Expected Param node");
        };

        if asterisk.is_some() {
            self.output.push('*');
        }
        self.append_display(node);

        if let (Some(eq_token), Some(default)) = (eq_token, default) {
            self.append_space();
            self.append_display(eq_token);
            self.append_space();
            if matches!(default.kind, mq_lang::CstNodeKind::Selector { .. }) {
                // Avoid a newline from the selector's leading trivia.
                self.format_selector(default, 0);
            } else {
                self.format_node(default, 0);
            }
        }
    }

    fn format_try(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        let mq_lang::CstNodeKind::Try {
            colon_or_do,
            body,
            catch,
        } = &node.kind
        else {
            unreachable!("Expected Try node");
        };

        if let Some(colon) = colon_or_do {
            self.append_display(colon);
            self.append_space();
        }

        self.format_indented_body(body, indent_level);

        if let Some(catch) = catch {
            self.format_node(catch, indent_level);
        }
    }

    fn format_catch(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if !node.has_new_line() {
            self.append_space();
        }

        self.append_indent(indent_level);
        self.append_display(node);

        let mq_lang::CstNodeKind::Catch {
            params,
            colon_or_do,
            body,
        } = &node.kind
        else {
            unreachable!("Expected Catch node");
        };

        for child in params.iter().flatten() {
            self.format_node(child, 0);
        }

        if let Some(colon) = colon_or_do {
            self.append_display(colon);
            self.append_space();
        }

        self.format_indented_body(body, indent_level);
    }

    fn format_indented_body(&mut self, body: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let child_indent = if body.has_new_line() {
            indent_level + 1
        } else {
            indent_level
        };
        self.format_node(body, child_indent);
    }

    fn format_match(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        indent_level: usize,
        block_indent_level: usize,
    ) {
        let is_prev_pipe = self.is_prev_pipe();
        self.append_indent(indent_level);
        self.append_display(node);
        self.append_space();

        let indent_level = if self.is_last_line_pipe() {
            block_indent_level
        } else {
            indent_level
        };

        let indent_adjustment = self.calculate_indent_adjustment();

        let mq_lang::CstNodeKind::Match {
            args,
            colon_or_do,
            arms,
            end_token,
        } = &node.kind
        else {
            unreachable!("Expected Match node");
        };

        for child in args.iter() {
            self.format_node(child, 0);
        }

        let remaining_children: Vec<&mq_lang::Shared<mq_lang::CstNode>> = arms.iter().chain(end_token.iter()).collect();

        if let Some(separator) = colon_or_do {
            if matches!(separator.kind, mq_lang::CstNodeKind::Do) {
                let mut rest = remaining_children.iter().copied().peekable();
                self.format_do_with_spacing(separator, &mut rest);
            } else {
                self.format_node(separator, 0);
            }
        }

        // Calculate indent level for match arms (similar to format_if)
        let node_indent_level = if is_prev_pipe {
            indent_level + 2
        } else {
            indent_level + 1
        } + indent_adjustment;

        // Calculate indent level for end keyword
        let end_indent_level = if is_prev_pipe { indent_level + 1 } else { indent_level } + indent_adjustment;

        // Check if this is a multiline match (first match arm has new line)
        let is_multiline = remaining_children
            .iter()
            .any(|child| matches!(child.kind, mq_lang::CstNodeKind::MatchArm { .. }) && child.has_new_line());

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
            self.format_node(child, node_indent_level);
        }
    }

    /// Formats a match arm node, handling pipe, pattern, optional guard, colon, and body.
    fn format_match_arm(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let mq_lang::CstNodeKind::MatchArm {
            pipe,
            pattern,
            if_token,
            guard_args,
            colon,
            body,
        } = &node.kind
        else {
            unreachable!("Expected MatchArm node");
        };

        if node.has_new_line() {
            self.append_indent(indent_level);
            self.output.push('|');
            self.append_space();
        } else {
            self.format_node(pipe, 0);
        }

        self.format_node(pattern, 0);

        if if_token.is_some() {
            self.append_space();
            self.output.push_str("if ");
            for expr in guard_args.iter().flatten() {
                self.format_node(expr, 0);
            }
        }

        if let Some(token) = colon.token.as_ref() {
            self.append_display(token);
            self.append_space();
        }

        self.format_node(body, indent_level + 1);
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
                mq_lang::TokenKind::BytesLiteral(_) => {
                    self.append_display(token);
                }
                // `Number`'s Display rounds to 6 decimals.
                mq_lang::TokenKind::NumberLiteral(n) => self.append_display(&n.value()),
                mq_lang::TokenKind::BoolLiteral(b) => self.append_display(b),
                mq_lang::TokenKind::None => self.append_display(token),
                mq_lang::TokenKind::Ident(name) => self.output.push_str(name),
                _ => {}
            }
        }

        // Format children (for complex patterns like arrays, dicts, type patterns)
        let children = node.all_children();
        if !children.is_empty() {
            // Check if this is a type pattern (starts with colon)
            if let Some(first) = children.first()
                && let Some(token) = &first.token
            {
                if matches!(token.kind, mq_lang::TokenKind::Colon) {
                    // Type pattern: :type_name
                    self.append_display(token);
                    if let Some(second) = children.get(1)
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

            for child in children.iter() {
                self.format_node(child, indent_level);
            }
        }
    }

    fn format_or_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        for child in node.children() {
            match &child.kind {
                mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. } => {
                    self.format_pattern(&mq_lang::Shared::clone(child), indent_level);
                }
                mq_lang::CstNodeKind::Token => {
                    if let Some(token) = &child.token
                        && matches!(token.kind, mq_lang::TokenKind::Or)
                    {
                        self.output.push_str(" || ");
                    }
                }
                _ => {}
            }
        }
    }

    fn format_array_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        for child in node.children() {
            match &child.kind {
                mq_lang::CstNodeKind::Token => {
                    if let Some(token) = &child.token {
                        match &token.kind {
                            mq_lang::TokenKind::Comma => self.output.push_str(", "),
                            _ => self.append_display(token),
                        }
                    }
                }
                _ => self.append_display(child),
            }

            child.children().for_each(|gc| {
                self.format_node(gc, 0);
            });
        }
    }

    fn format_dict_pattern(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        let mut i = 0;
        let children = node.all_children();

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
                            self.append_display(next_token);
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
                        self.append_display(token);
                        i += 1;
                        continue;
                    }
                }
            }

            child.children().for_each(|gc| {
                self.format_node(gc, 0);
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

                    self.append_display(comment);
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
            self.append_display(token);
        }
    }

    fn append_literal(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
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
                mq_lang::TokenKind::BytesLiteral(_) => {
                    self.append_display(token);
                }
                // `Number`'s Display rounds to 6 decimals.
                mq_lang::TokenKind::NumberLiteral(n) => self.append_display(&n.value()),
                mq_lang::TokenKind::BoolLiteral(b) => self.append_display(b),
                mq_lang::TokenKind::None => self.append_display(token),
                other => {
                    eprintln!(
                        "Warning: Unexpected token kind in append_literal: {:?}. Inserting placeholder.",
                        other
                    );
                    self.append_display(other);
                }
            }
        }
    }

    fn append_symbol(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        // Format symbol as :ident or :"string"
        let mq_lang::CstNodeKind::Symbol { name, .. } = &node.kind else {
            unreachable!("Expected Symbol node");
        };
        self.output.push(':');
        if let Some(token) = &name.token {
            match &token.kind {
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
            kind:
                mq_lang::CstNodeKind::Selector { .. }
                | mq_lang::CstNodeKind::SelfAttr
                | mq_lang::CstNodeKind::SelectorCall { .. },
            token: Some(token),
            ..
        } = &**node
        {
            self.append_indent(indent_level);
            self.append_display(token);

            node.children().for_each(|child| {
                self.format_node(child, indent_level);
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
                        self.append_display(token);
                    } else {
                        if !self.output.ends_with(' ') {
                            self.append_space();
                        }
                        self.append_display(token);
                    }
                }
                mq_lang::TokenKind::Do => {
                    if node.has_new_line() {
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.append_display(token);
                    } else {
                        if !self.output.ends_with(' ') {
                            self.append_space();
                        }
                        self.append_display(token);
                    }
                }
                _ => {
                    self.append_indent(indent_level);
                    self.append_display(token);
                }
            }
        }
    }

    fn format_break(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        // Format children (colon and expression if present)
        for child in node.children() {
            self.format_node(child, 0);
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
                        self.append_display(token);
                    } else {
                        self.append_display(token);
                        self.output.push(' ');
                    }
                }
                mq_lang::TokenKind::Colon => self.append_display(token),
                mq_lang::TokenKind::Equal => {
                    self.output.push(' ');
                    self.append_display(token);
                    self.output.push(' ');
                }
                mq_lang::TokenKind::Pipe => {
                    let force_wrap = !node.has_new_line() && self.should_wrap_pipe();

                    if node.has_new_line() || force_wrap {
                        self.append_leading_trivia(node, indent_level);

                        if force_wrap {
                            self.append_newline();
                        }

                        self.append_indent(indent_level);
                        self.append_display(token);
                        self.output.push(' ');
                    } else {
                        self.output.push(' ');
                        self.append_display(token);
                        self.output.push(' ');
                    }
                }
                mq_lang::TokenKind::RParen => {
                    if node.has_new_line() {
                        let indent_level = indent_level.saturating_sub(1);
                        self.append_leading_trivia(node, indent_level);
                        self.append_indent(indent_level);
                        self.append_display(token);
                    } else {
                        self.append_display(token);
                    }
                }
                _ => self.append_display(token),
            }
        }
    }

    #[inline(always)]
    fn append_display(&mut self, value: &impl std::fmt::Display) {
        let _ = write!(self.output, "{value}");
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
    fn current_line_width(&self) -> usize {
        let start = self.output.rfind('\n').map_or(0, |pos| pos + 1);
        self.output[start..].chars().count()
    }

    #[inline(always)]
    fn should_wrap_pipe(&self) -> bool {
        self.config
            .max_width
            .is_some_and(|max_width| self.current_line_width() >= max_width)
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
            let trimmed = last_line.trim();
            (!last_line.starts_with("let ") && trimmed.starts_with("let "))
                || trimmed
                    .strip_prefix('|')
                    .is_some_and(|rest| rest.chars().filter(|c| *c != ' ').take(3).eq("let".chars()))
        } else {
            false
        }
    }

    fn find_token_position<F>(node: &mq_lang::Shared<mq_lang::CstNode>, token_kind_matcher: F) -> Option<usize>
    where
        F: Fn(&mq_lang::TokenKind) -> bool,
    {
        node.children().position(|c| {
            c.token
                .as_ref()
                .map(|token| token_kind_matcher(&token.kind))
                .unwrap_or(false)
        })
    }

    #[inline(always)]
    fn calculate_indent_adjustment(&self) -> usize {
        if self.is_let_line() {
            self.current_line_indent()
        } else {
            0
        }
    }

    fn format_colon_with_spacing<'a, I>(
        &mut self,
        colon_node: &mq_lang::Shared<mq_lang::CstNode>,
        remaining: &mut std::iter::Peekable<I>,
        indent_level: usize,
    ) where
        I: Iterator<Item = &'a mq_lang::Shared<mq_lang::CstNode>>,
    {
        self.format_node(colon_node, indent_level);

        if let Some(next) = remaining.peek()
            && !next.has_new_line()
            && !matches!(next.kind, mq_lang::CstNodeKind::Block { .. })
        {
            self.append_space();
        }
    }

    /// Formats a 'do' keyword with standardized spacing.
    /// Adds space before the 'do' keyword, formats it, and adds space after
    /// if the next node doesn't have a newline and is not a MatchArm.
    fn format_do_with_spacing<'a, I>(
        &mut self,
        do_node: &mq_lang::Shared<mq_lang::CstNode>,
        remaining: &mut std::iter::Peekable<I>,
    ) where
        I: Iterator<Item = &'a mq_lang::Shared<mq_lang::CstNode>>,
    {
        // Add space before 'do' keyword
        self.append_space();

        // Format the 'do' keyword
        self.format_node(do_node, 0);

        // Add space after 'do' if next node doesn't have newline and is not a MatchArm
        if let Some(next) = remaining.peek()
            && !next.has_new_line()
            && !matches!(next.kind, mq_lang::CstNodeKind::MatchArm { .. })
        {
            self.append_space();
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
    #[case::loop_multiline(
        "loop:
        process();",
        "loop:
  process();
"
    )]
    #[case::loop_oneline("loop: process();", "loop: process();")]
    #[case::loop_with_pipe(
        "loop:
        data
        | process()
        | output();",
        "loop:
  data
  | process()
  | output();
"
    )]
    #[case::loop_with_break(
        "loop:
        add(self,1)
        |if(gt(self,5)):break;;",
        "loop:
  add(self, 1)
  | if (gt(self, 5)): break;;
"
    )]
    #[case::until_multiline(
        "until(condition()):
        process();",
        "until (condition()):
  process();
"
    )]
    #[case::until_oneline("until(condition()): process();", "until (condition()): process();")]
    #[case::until_with_pipe(
        "until(check_condition()):
        data
        | process()
        | output();",
        "until (check_condition()):
  data
  | process()
  | output();
"
    )]
    #[case::until_with_break(
        "until(false):
        add(self,1)
        |if(gt(self,5)):break;;",
        "until (false):
  add(self, 1)
  | if (gt(self, 5)): break;;
"
    )]
    #[case::unless_multiline(
        "unless(test):
        test",
        "unless (test):
  test
"
    )]
    #[case::unless_oneline("unless(test): test", "unless (test): test")]
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
    #[case::import("import  \"test.mq\"", "import \"test.mq\"")]
    #[case::import_as("import  \"test.mq\"   as   m", "import \"test.mq\" as m")]
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
    #[case::array_trailing_comma("[1, 2,]", "[1, 2,]")]
    #[case::dict_trailing_comma("{\"a\": 1,}", "{\"a\": 1,}")]
    #[case::dict_trailing_comma_multiline("{\n  \"a\": 1,\n  \"b\": 2,\n}", "{\n  \"a\": 1,\n  \"b\": 2,\n}\n")]
    #[case::array_comment_before_close("[\n  1,\n  # c\n]", "[\n  1,\n  # c\n]\n")]
    #[case::dict_comment_before_close("{\n  \"a\": 1\n  # c\n}", "{\n  \"a\": 1\n  # c\n}\n")]
    #[case::array_same_line_comment_before_close("[1, 2, # c\n]", "[1, 2, # c\n]\n")]
    #[case::array_comments_before_close("[\n  1,\n\n  # a\n  # b\n]", "[\n  1,\n  # a\n  # b\n]\n")]
    #[case::number_small_fraction("1e-9", "0.000000001")]
    #[case::number_long_fraction("3.14159265358979", "3.14159265358979")]
    #[case::number_integral_float("2.0", "2")]
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
    #[case::standalone_attr_selector(".lang", ".lang")]
    #[case::standalone_attr_selector_value(".value", ".value")]
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
    #[case::try_catch_with_binder(
        "try: process() catch(e): handle_error(e)",
        "try: process() catch(e): handle_error(e)"
    )]
    #[case::try_catch_with_binder_extra_spaces(
        "try: process() catch( e ): handle_error(e)",
        "try: process() catch(e): handle_error(e)"
    )]
    #[case::try_catch_with_binder_multiline(
        r#"try:
  process()
catch(e):
  handle_error(e)"#,
        "try:
  process()
catch(e):
  handle_error(e)
"
    )]
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
    #[case::module_with_do(
        r#"module test  do
import "foo.mq"
| def main(): test();
end"#,
        r#"module test do
  import "foo.mq"
  | def main(): test();
end
"#
    )]
    #[case::module_with_do_and_new_line(
        r#"module test
do
import "foo.mq"
| def main(): test();
end"#,
        r#"module test
  do
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
    #[case::assign_right_multiline(
        r#"let x =
"test""#,
        r#"let x =
  "test"
"#
    )]
    #[case::assign_right_multiline_pipe(
        r#"let x =
"test"
| upcase()"#,
        r#"let x =
  "test"
| upcase()
"#
    )]
    #[case::assign_right_multiline_if(
        r#"let x =
if(test):
test
else:
test2"#,
        r#"let x =
  if (test):
    test
  else:
    test2
"#
    )]
    #[case::if_without_colon_oneline("if(test) 1 else 2", "if (test) 1 else 2")]
    #[case::if_without_colon_multiline(
        r#"if(test)
test
else
test2"#,
        r#"if (test)
  test
else
  test2
"#
    )]
    #[case::if_elif_else_without_colon_oneline("if(test) 1 elif(test2) 2 else 3", "if (test) 1 elif (test2) 2 else 3")]
    #[case::if_elif_else_without_colon_multiline(
        r#"if(test)
test
elif(test2)
test2
else
test3"#,
        r#"if (test)
  test
elif (test2)
  test2
else
  test3
"#
    )]
    #[case::if_without_colon_with_do_block(
        r#"if(test) do
test
end
else do
test2
end"#,
        r#"if (test) do
    test
  end
else do
    test2
  end
"#
    )]
    #[case::while_do_end_oneline("while(x > 0) do x - 1 end", "while (x > 0) do x - 1 end")]
    #[case::while_do_end_multiline(
        r#"while(x > 0) do
let x = x - 1 | x
end"#,
        r#"while (x > 0) do
  let x = x - 1 | x
end
"#
    )]
    #[case::while_do_end_with_break(
        r#"while(x < 10) do
let x = x + 1
| if(x == 3):
break
else:
x
end"#,
        r#"while (x < 10) do
  let x = x + 1
  | if (x == 3):
      break
    else:
      x
end
"#
    )]
    #[case::foreach_do_end_oneline(
        "foreach(item, arr) do process(item) end",
        "foreach (item, arr) do process(item) end"
    )]
    #[case::foreach_do_end_multiline(
        r#"foreach(x, array(1, 2, 3)) do
add(x, 1)
end"#,
        r#"foreach (x, array(1, 2, 3)) do
  add(x, 1)
end
"#
    )]
    #[case::foreach_do_end_with_continue(
        r#"foreach(x, array(1, 2, 3, 4, 5)) do
if(x == 3):
continue
else:
x + 10
end"#,
        r#"foreach (x, array(1, 2, 3, 4, 5)) do
  if (x == 3):
    continue
  else:
    x + 10
end
"#
    )]
    #[case::foreach_do_end_nested(
        r#"foreach(row, arr) do
foreach(x, row) do
x * 2
end
end"#,
        r#"foreach (row, arr) do
  foreach (x, row) do
    x * 2
  end
end
"#
    )]
    #[case::match_do_end_oneline(
        "match(x) do | 1: \"one\" | 2: \"two\" | _: \"other\" end",
        "match (x) do | 1: \"one\" | 2: \"two\" | _: \"other\" end"
    )]
    #[case::match_do_end_multiline(
        r#"match(2) do
| 1: "one"
| 2: "two"
| _: "other"
end"#,
        r#"match (2) do
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
    )]
    #[case::match_do_end_type_pattern(
        r#"match(array(1, 2, 3)) do
| :array: "is_array"
| :number: "is_number"
| _: "other"
end"#,
        r#"match (array(1, 2, 3)) do
  | :array: "is_array"
  | :number: "is_number"
  | _: "other"
end
"#
    )]
    #[case::match_do_end_with_guard(
        r#"match(x) do
| n if(n > 0): "positive"
| _: "non-positive"
end"#,
        r#"match (x) do
  | n if (n > 0): "positive"
  | _: "non-positive"
end
"#
    )]
    #[case::def_with_default_single(
        "def greet(name = \"World\"):
        name;",
        "def greet(name = \"World\"):
  name;
"
    )]
    #[case::def_with_multiple_defaults(
        "def foo(a, b = 2, c = 3):
        a + b + c;",
        "def foo(a, b = 2, c = 3):
  a + b + c;
"
    )]
    #[case::def_with_mixed_params(
        "def bar(x,y=10):
        x+y;",
        "def bar(x, y = 10):
  x + y;
"
    )]
    #[case::def_with_array_default(
        "def baz(x,y=[1,2,3]):
        x+len(y);",
        "def baz(x, y = [1, 2, 3]):
  x + len(y);
"
    )]
    #[case::def_with_ident_default(
        "def f(x = y):
        x;",
        "def f(x = y):
  x;
"
    )]
    #[case::def_with_string_default(
        "def test(a,b=\"test\"):
        a+b;",
        "def test(a, b = \"test\"):
  a + b;
"
    )]
    #[case::def_with_number_default(
        "def calc(x=42):
        x * 2;",
        "def calc(x = 42):
  x * 2;
"
    )]
    #[case::def_with_boolean_default(
        "def result(x=true):
        x;",
        "def result(x = true):
  x;
"
    )]
    #[case::def_all_params_with_defaults(
        "def calc(a=1,b=2,c=3):
        a+b+c;",
        "def calc(a = 1, b = 2, c = 3):
  a + b + c;
"
    )]
    #[case::def_expr_with_default(
        "def calc(a=1,b=2,c=3 + 1):
        a+b+c;",
        "def calc(a = 1, b = 2, c = 3 + 1):
  a + b + c;
"
    )]
    #[case::def_expr_with_variadic(
        "def calc(v,*args):
        v + args;",
        "def calc(v, *args):
  v + args;
"
    )]
    #[case::assign_with_selector_attr("value.test|= value.attr", "value.test |= value.attr")]
    #[case::index_assign("arr[0]=10", "arr[0] = 10")]
    #[case::index_assign_string_key("dict[\"key\"]=\"value\"", "dict[\"key\"] = \"value\"")]
    #[case::index_compound_assign("arr[0]+=1", "arr[0] += 1")]
    #[case::selector_call_heading_single_arg(".h(1)", ".h(1)")]
    #[case::selector_call_heading_multi_arg(".h(1,2)", ".h(1, 2)")]
    #[case::selector_call_code_lang(".code(\"rust\")", ".code(\"rust\")")]
    #[case::selector_call_pipe(".h(1)|.text()", ".h(1) | .text()")]
    #[case::descendant_chain(".blockquote .code", ".blockquote .code")]
    #[case::descendant_chain_no_space(".blockquote.code", ".blockquote .code")]
    #[case::descendant_chain_extra_space(".blockquote  .code", ".blockquote .code")]
    #[case::descendant_chain_three_levels(".blockquote .list .code", ".blockquote .list .code")]
    #[case::descendant_chain_with_selector_call(".blockquote .code(\"rust\")", ".blockquote .code(\"rust\")")]
    #[case::descendant_chain_trailing_attr(".blockquote .code.lang", ".blockquote .code.lang")]
    #[case::arrow_simple("->(): program;", "->(): program;")]
    #[case::arrow_multiline(
        "->(arg1,arg2):
        program;",
        "->(arg1, arg2):
  program;
"
    )]
    #[case::arrow_as_call_arg("map( ->():program;)", "map(->(): program;)")]
    #[case::arrow_as_call_arg_single_line("map(->(): program;)", "map(->(): program;)")]
    #[case::arrow_as_call_arg_with_other_args("map(->(): program;, 1, \"test\")", "map(->(): program;, 1, \"test\")")]
    #[case::nested_arrow_as_call_arg("outer(map(->(): inner();))", "outer(map(->(): inner();))")]
    #[case::arrow_end("->(): test end", "->(): test end")]
    #[case::bytes_literal_basic(r#"b"abc""#, r#"b"abc""#)]
    #[case::bytes_literal_hex(r#"b"\xf0\x9f\x99\x82""#, r#"b"\xf0\x9f\x99\x82""#)]
    #[case::bytes_literal_with_pipe(r#"b"abc"  |  len"#, r#"b"abc" | len"#)]
    #[case::bytes_literal_in_call(r#"len(b"abc")"#, r#"len(b"abc")"#)]
    #[case::as_binding_basic("42 as x | x", "42 as x | x")]
    #[case::as_binding_spaces("42  as  x  |  x", "42 as x | x")]
    #[case::as_binding_selector(".text as title | title", ".text as title | title")]
    #[case::def_with_do_block_body(
        r#"def test(x):
  do
    step1(x)
    | step2(x)
  end;"#,
        r#"def test(x):
  do
    step1(x)
    | step2(x)
  end;
"#
    )]
    #[case::foreach_colon_with_do_block_body(
        r#"foreach(x, items):
  do
    process(x)
  end;"#,
        r#"foreach (x, items):
  do
    process(x)
  end;
"#
    )]
    #[case::foreach_do_with_nested_do_block(
        r#"foreach(row, arr) do
  do
    process(row)
  end
end"#,
        r#"foreach (row, arr) do
  do
    process(row)
  end
end
"#
    )]
    #[case::fn_multiline_params_as_first_array_element(
        "let fns = [fn(
  a,
  b
): a + b;, fn(c): c;]",
        "let fns = [fn(
  a,
  b
): a + b;, fn(c): c;]
"
    )]
    #[case::fn_multiline_variadic_params_as_first_array_element(
        "let fns = [fn(
  a,
  *rest
): rest;, fn(c): c;]",
        "let fns = [fn(
  a,
  *rest
): rest;, fn(c): c;]
"
    )]
    #[case::fn_multiline_params_as_non_first_array_element(
        "let fns = [first, fn(
  a,
  b
): a + b;]",
        "let fns = [first, fn(
  a,
  b
): a + b;]
"
    )]
    #[case::fn_multiline_params_in_nested_array(
        "let xs = [[fn(
  a,
  b
): a + b;]]",
        "let xs = [[fn(
  a,
  b
): a + b;]]
"
    )]
    #[case::fn_multiline_params_as_call_arg(
        "map(arr, fn(
  a,
  b
): a + b;)",
        "map(arr, fn(
  a,
  b
): a + b;)
"
    )]
    #[case::qualified_call_multiline_args_nested_in_call(
        "let result = to_string(md::doc(
    md::h(\"Items\", 2),
    map([\"a\", \"b\"], fn(x): md::list(x);),
  ))",
        "let result = to_string(md::doc(
  md::h(\"Items\", 2),
  map([\"a\", \"b\"], fn(x): md::list(x);),
))
"
    )]
    #[case::yield_oneline("def g(): yield: 1;", "def g(): yield:1;")]
    #[case::yield_bare("def g(): yield;", "def g(): yield;")]
    #[case::yield_multiline(
        "def g():
        yield: 1
        | yield: 2;",
        "def g():
  yield:1
  | yield:2;
"
    )]
    #[case::def_with_comment_before_yield(
        "def g():
        # a comment
        yield: 1;",
        "def g():
  # a comment
  yield:1;
"
    )]
    fn test_format(#[case] code: &str, #[case] expected: &str) {
        let result = Formatter::new(None).format(code);
        assert_eq!(result.unwrap(), expected);
    }

    #[rstest]
    #[case::sort_imports(
        r#"import "c.mq"
| import "a.mq"
| import "b.mq""#,
        r#"import "a.mq"
| import "b.mq"
| import "c.mq"
"#
    )]
    #[case::sort_imports_with_alias(
        r#"import "c.mq"
| import "a.mq" as a
| import "b.mq""#,
        r#"import "a.mq" as a
| import "b.mq"
| import "c.mq"
"#
    )]
    #[case::sort_functions(
        r#"def z(): test;
def a(): test;
def m(): test;
"#,
        "def a(): test;\ndef m(): test;\ndef z(): test;\n"
    )]
    #[case::sort_fields(
        r#"let z = 1
| let a = 2
| let m = 3"#,
        r#"let a = 2
| let m = 3
| let z = 1
"#
    )]
    #[case::sort_mixed(
        r#"let z = 1
| import "b.mq"
def y(): test;
| let a = 2
| import "a.mq"
def b(): test;"#,
        "import \"a.mq\"\n| import \"b.mq\"\n| let a = 2\n| let z = 1\n|\ndef b(): test;\ndef y(): test;\n"
    )]
    #[case::sort_with_other(
        r#"def z(): test;
| let x = 1
| nodes
def a(): test;"#,
        "let x = 1\n|\ndef a(): test;\ndef z(): test;\n| nodes\n"
    )]
    #[case::sort_with_piped_calls(
        r#"def z(): test;
| let x = 1
| add() | mul() | sub()
def a(): test;
| let y = 2"#,
        "let x = 1\n| let y = 2\n|\ndef a(): test;\ndef z(): test;\n| add()\n| mul()\n| sub()\n"
    )]
    #[case::sort_all_types(
        r#"add() | sub()
def func_b(): test;
| let var_z = 1
| import "z.mq"
| let var_a = 2
def func_a(): test;
| mul() | div()
| import "a.mq""#,
        "import \"a.mq\"\n| import \"z.mq\"\n| let var_a = 2\n| let var_z = 1\n|\ndef func_a(): test;\ndef func_b(): test;\n| add()\n| sub()\n| mul()\n| div()\n"
    )]
    fn test_format_with_sort(#[case] code: &str, #[case] expected: &str) {
        let config = FormatterConfig {
            indent_width: 2,
            sort_imports: true,
            sort_functions: true,
            sort_fields: true,
            max_width: None,
        };
        let result = Formatter::new(Some(config)).format(code);
        assert_eq!(result.unwrap(), expected);
    }

    #[rstest]
    #[case::wraps_top_level_pipeline(
        "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\")",
        40,
        "select(\"h1\") | upcase() | add_class(\"title\")\n| trim() | to_text() | replace(\"a\", \"b\")\n| join(\",\")\n"
    )]
    #[case::wraps_pipeline_inside_def(
        "def foo(x):\n  select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\");",
        40,
        "def foo(x):\n  select(\"h1\") | upcase() | add_class(\"title\")\n  | trim() | to_text() | replace(\"a\", \"b\")\n  | join(\",\");\n"
    )]
    #[case::keeps_short_pipeline_inline("select(\"h1\") | upcase()", 80, "select(\"h1\") | upcase()")]
    fn test_format_with_max_width(#[case] code: &str, #[case] max_width: usize, #[case] expected: &str) {
        let config = FormatterConfig {
            max_width: Some(max_width),
            ..Default::default()
        };
        let result = Formatter::new(Some(config)).format(code);
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_format_with_max_width_is_idempotent() {
        let code = "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\")";
        let config = || FormatterConfig {
            max_width: Some(40),
            ..Default::default()
        };
        let once = Formatter::new(Some(config())).format(code).unwrap();
        let twice = Formatter::new(Some(config())).format(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn test_format_without_max_width_does_not_wrap() {
        let code = "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\", \"b\") | join(\",\")";
        let result = Formatter::default().format(code);
        assert_eq!(result.unwrap(), code);
    }

    #[test]
    fn test_format_yield_is_idempotent() {
        for code in ["def g(): yield: 1;", "def g(): yield: 1 | yield: 2;"] {
            let once = Formatter::new(None).format(code).unwrap();
            let twice = Formatter::new(None).format(&once).unwrap();
            assert_eq!(once, twice, "not idempotent for {code:?}");
        }
    }

    const FORMAT_STATEMENTS: &[&str] = &[
        "def f(x):\n  x + 1;",
        "let a = [1,2]",
        "if(a):1 elif(b):2 else:3",
        "foreach(v,a):v;",
        "match(x): | 1: :a | _: :b end",
        "{\"k\": 1}",
        ".h1",
        "try: 1 catch(e): e",
        "fn(x): x;",
        "while(a): break;",
        "a[0](1)",
        "1 as n",
    ];

    /// The previous allocating `is_let_line`, kept as an oracle.
    fn reference_is_let_line(output: &str) -> bool {
        let start = output.rfind('\n').map_or(0, |pos| pos + 1);
        if start < output.len() {
            let last_line = &output[start..];
            (!last_line.starts_with("let ") && last_line.trim().starts_with("let "))
                || last_line.trim().replace(" ", "").starts_with("|let")
        } else {
            false
        }
    }

    fn is_let_line(output: &str) -> bool {
        let mut formatter = Formatter::new(None);
        formatter.output = output.to_string();
        formatter.is_let_line()
    }

    #[rstest]
    #[case::top_level_let("let x = 1", false)]
    #[case::indented_let("  let x = 1", true)]
    #[case::piped_let("| let x", true)]
    #[case::piped_let_no_space("|let x", true)]
    #[case::piped_let_spaced("|   let x", true)]
    #[case::last_line_only("let a = 1\n  | let b", true)]
    #[case::previous_line_ignored("  let a\nfoo", false)]
    #[case::trailing_newline("  let a\n", false)]
    #[case::empty("", false)]
    #[case::not_let("  letter", false)]
    #[case::pipe_other("| foo", false)]
    fn test_is_let_line(#[case] output: &str, #[case] expected: bool) {
        assert_eq!(is_let_line(output), expected);
        assert_eq!(reference_is_let_line(output), expected);
    }

    proptest::proptest! {
        #[test]
        fn prop_is_let_line_matches_reference(output in "[ |let\\na-z\\t]{0,24}") {
            proptest::prop_assert_eq!(is_let_line(&output), reference_is_let_line(&output));
        }

        /// Formatting through a pre-parsed CST matches formatting from source.
        #[test]
        fn prop_format_with_cst_matches_format(
            stmts in proptest::collection::vec(proptest::sample::select(FORMAT_STATEMENTS), 1..6)
        ) {
            let code = stmts.join(" | ");
            let expected = Formatter::new(None).format(&code).unwrap();
            let (mut nodes, _) = mq_lang::parse_recovery(&code);
            let actual = Formatter::new(None).format_with_cst(&mut nodes).unwrap();
            proptest::prop_assert_eq!(actual, expected);
        }

        #[test]
        fn prop_format_is_idempotent(
            stmts in proptest::collection::vec(proptest::sample::select(FORMAT_STATEMENTS), 1..6)
        ) {
            let once = Formatter::new(None).format(&stmts.join(" | ")).unwrap();
            let twice = Formatter::new(None).format(&once).unwrap();
            proptest::prop_assert_eq!(twice, once);
        }
    }
}
