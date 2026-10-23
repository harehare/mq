use std::fmt::Write;

use mq_lang::{CstNode, CstNodeKind};

#[derive(Clone, Debug, Default)]
/// Formats mq source code according to a [`FormatterConfig`].
pub struct Formatter {
    config: FormatterConfig,
    output: String,
    indent_cache: Vec<String>,
    source: Option<Source>,
}

/// The input being formatted, used to reproduce literals exactly as written.
#[derive(Clone, Debug, Default)]
struct Source {
    text: String,
    line_starts: Vec<usize>,
}

impl Source {
    fn new(text: &str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self {
            text: text.to_string(),
            line_starts,
        }
    }

    /// Converts a 1-based line and 1-based char column into a byte offset.
    fn offset(&self, pos: &mq_lang::Position) -> Option<usize> {
        let line_start = *self.line_starts.get((pos.line as usize).checked_sub(1)?)?;
        let line = &self.text[line_start..];
        let column = pos.column.checked_sub(1)?;
        let byte = line.char_indices().nth(column).map(|(i, _)| i).unwrap_or(line.len());
        Some(line_start + byte)
    }

    fn slice(&self, range: &mq_lang::Range) -> Option<&str> {
        self.text.get(self.offset(&range.start)?..self.offset(&range.end)?)
    }
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
/// Formatting options.
pub struct FormatterConfig {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Sort `import`/`include` statements.
    pub sort_imports: bool,
    /// Sort top-level function definitions.
    pub sort_functions: bool,
    /// Sort dict fields.
    pub sort_fields: bool,
    /// Maximum line width before wrapping, if any.
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
    /// Creates a formatter; `None` uses [`FormatterConfig::default`].
    pub fn new(config: Option<FormatterConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            output: String::new(),
            indent_cache: Vec::new(),
            source: None,
        }
    }

    /// Formats `code`, returning the CST error reporter if it fails to parse.
    pub fn format(&mut self, code: &str) -> Result<String, mq_lang::CstErrorReporter> {
        if code.is_empty() {
            return Ok(String::new());
        }

        let (mut nodes, errors) = mq_lang::parse_recovery(code);

        if errors.has_errors() {
            return Err(errors);
        }

        self.source = Some(Source::new(code));
        let result = self.format_with_cst(&mut nodes);
        self.source = None;
        result
    }

    /// Formats an already parsed CST. Without the source text, string escapes such as
    /// `\u{41}` are normalized to their decoded form; use [`Formatter::format`] to keep them.
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
        let has_leading_new_line = node.has_new_line()
            || Self::nameless_call_head_has_new_line(node)
            || (node.is_pipe() && self.should_wrap_pipe());
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
            mq_lang::CstNodeKind::Eof | mq_lang::CstNodeKind::Missing { .. } => {}
            mq_lang::CstNodeKind::Error { .. } => {
                self.separate_error_node(node);
                self.append_raw_children(node);
            }
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

        let indent_adjustment = if indent_level == 0 {
            // Inline: children nest under the current line, counting a leading `| `.
            self.current_line_content_indent()
        } else if self.is_let_line() || self.is_last_line_pipe() {
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
        let indent_adjustment = if indent_level == 0 {
            // Inline: children nest under the current line, counting a leading `| `.
            self.current_line_content_indent()
        } else if self.is_let_line() || self.is_last_line_pipe() {
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
                    self.append_comment(comment);
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

        // A `Missing` closer has no text, and its comments were already written above.
        if matches!(last.kind, mq_lang::CstNodeKind::Missing { .. }) {
            if after_newline && !is_multiline && !has_comment {
                self.append_newline();
            }
        } else {
            self.format_node(last, indent_level);
        }
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
        // Inline, the base below already comes from the current line.
        let indent_adjustment = if indent_level == 0 {
            0
        } else {
            self.calculate_indent_adjustment()
        };

        if node.has_new_line() {
            self.append_indent(indent_level);
        }
        self.append_display(node);

        // Re-derive from the actual line when inline, since the parent's
        // block_indent_level may not match where this node was written.
        let block_indent_level = if indent_level != 0 {
            block_indent_level
        } else if is_prev_pipe {
            self.current_line_indent()
        } else {
            self.current_line_content_indent()
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

        // `..h` and `.."key"` are written without a space.
        if prev
            .token
            .as_ref()
            .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::DoubleDot))
        {
            return false;
        }

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
        let inline_let_indent = (indent_level == 0 && self.is_let_line()).then(|| self.current_line_content_indent());

        // A tokenless block is an implicit pipe chain and has no opening token to space around.
        let has_open_token = node.token.is_some();

        if node.has_new_line() {
            self.append_indent(indent_level);
        } else if has_open_token && !self.output.is_empty() && !self.output.ends_with(' ') {
            self.append_space();
        }

        self.append_display(node);
        if has_open_token {
            self.append_space();
        }

        let all_children = node.all_children();
        let expr_nodes = all_children.iter().peekable();

        let base_indent = if indent_level > 0 {
            indent_level
        } else {
            inline_let_indent.unwrap_or(block_indent_level)
        };

        let child_indent_level = if is_prev_pipe { base_indent + 2 } else { base_indent + 1 };

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

    /// A bracket access on a non-identifier head (`"abc"[1]`) is a `Call` without a token whose
    /// first child is the head, so a newline before the expression sits on that child.
    fn nameless_call_head_has_new_line(node: &mq_lang::CstNode) -> bool {
        matches!(node.kind, mq_lang::CstNodeKind::Call { .. })
            && node.token.is_none()
            && node.children().next().is_some_and(|head| head.has_new_line())
    }

    fn format_call(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        let head_has_new_line = Self::nameless_call_head_has_new_line(node);
        if !head_has_new_line {
            self.append_indent(indent_level);
        }
        self.append_display(node);

        let current_line_indent = if indent_level == 0 {
            self.current_line_content_indent()
        } else {
            indent_level
        };

        node.children().enumerate().for_each(|(i, child)| {
            if i == 0 && head_has_new_line {
                self.format_node(child, indent_level);
                return;
            }
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

        let (indent_level, indent_adjustment) = if indent_level == 0 && self.is_let_line() {
            (self.current_line_content_indent(), 0)
        } else if self.is_last_line_pipe() {
            (block_indent_level, self.calculate_indent_adjustment())
        } else {
            (indent_level, self.calculate_indent_adjustment())
        };

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
            self.format_attribute(attr, indent_level);
        }
    }

    /// Formats the attribute selector that follows a value (`x.level`). Plain whitespace and newlines
    /// before it are dropped, but a comment is kept on its own line.
    fn format_attribute(&mut self, attr: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        if attr.leading_trivia.iter().any(|trivia| trivia.is_comment()) {
            self.format_node(attr, indent_level + 1);
        } else {
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

        let (indent_level, indent_adjustment) = if indent_level == 0 && self.is_let_line() {
            (self.current_line_content_indent(), 0)
        } else if self.is_last_line_pipe() {
            (block_indent_level, self.calculate_indent_adjustment())
        } else {
            (indent_level, self.calculate_indent_adjustment())
        };

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
                mq_lang::TokenKind::StringLiteral(s) => self.append_string_literal(token, s),
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

    /// Keeps an error node apart from the preceding token; the space between them is
    /// trailing trivia of the previous node, which the formatter does not emit.
    fn separate_error_node(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        let attaches = |c: char| c.is_whitespace() || matches!(c, '(' | '[' | '{');
        let first_attaches = node
            .children()
            .next()
            .and_then(|c| c.token.as_ref())
            .is_some_and(|token| {
                matches!(
                    token.kind,
                    mq_lang::TokenKind::Comma
                        | mq_lang::TokenKind::SemiColon
                        | mq_lang::TokenKind::RParen
                        | mq_lang::TokenKind::RBracket
                        | mq_lang::TokenKind::RBrace
                )
            });

        if !first_attaches && self.output.chars().last().is_some_and(|c| !attaches(c)) {
            self.output.push(' ');
        }
    }

    /// Emits an error node's children as written, since unparsable source has no formatting rules.
    fn append_raw_children(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>) {
        for child in node.children() {
            for trivia in &child.leading_trivia {
                self.append_raw_trivia(trivia);
            }
            if let Some(token) = &child.token {
                self.append_raw_token(token);
            }
            self.append_raw_children(child);
            for trivia in &child.trailing_trivia {
                self.append_raw_trivia(trivia);
            }
        }
    }

    fn append_raw_trivia(&mut self, trivia: &mq_lang::CstTrivia) {
        match trivia {
            mq_lang::CstTrivia::Comment(_) => self.append_comment(trivia),
            _ => self.append_display(trivia),
        }
    }

    /// Writes `token` so that it re-lexes to itself (`Display` drops string quotes).
    fn append_raw_token(&mut self, token: &mq_lang::Token) {
        match &token.kind {
            mq_lang::TokenKind::StringLiteral(s) => self.append_string_literal(token, s),
            mq_lang::TokenKind::NumberLiteral(n) => self.append_display(&n.value()),
            mq_lang::TokenKind::InterpolatedString(_) => {
                self.output.push_str("s\"");
                let escaped = token
                    .to_string()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\t', "\\t")
                    .replace('\r', "\\r");
                self.output.push_str(&escaped);
                self.output.push('"');
            }
            _ => self.append_display(token),
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

                    self.append_comment(comment);
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
                mq_lang::TokenKind::StringLiteral(s) => self.append_string_literal(token, s),
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
                mq_lang::TokenKind::StringLiteral(s) => self.append_string_literal(token, s),
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
            if let Some(raw) = self.raw_literal(token, "s\"") {
                self.output.push_str("s\"");
                Self::push_escaping_controls(&mut self.output, &raw, true);
                self.output.push('"');
                return;
            }
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

                    if let mq_lang::CstNodeKind::Self_ { attr: Some(attr) } = &node.kind {
                        // `. .level` keeps its space, since `..level` would be a different token.
                        if matches!(&token.kind, mq_lang::TokenKind::Selector(s) if s == ".") {
                            self.append_space();
                        }
                        self.format_attribute(attr, indent_level);
                    }
                }
            }
        }
    }

    fn format_break(&mut self, node: &mq_lang::Shared<mq_lang::CstNode>, indent_level: usize) {
        self.append_indent(indent_level);
        self.append_display(node);

        let (CstNodeKind::Break { colon, value } | CstNodeKind::Yield { colon, value }) = &node.kind else {
            return;
        };

        if let Some(colon) = colon {
            self.format_node(colon, 0);
        }

        if let Some(value) = value {
            if colon.is_some() && !value.has_new_line() {
                self.output.push(' ');
            }
            self.format_node(value, 0);
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
                mq_lang::TokenKind::SemiColon => {
                    if node.leading_trivia.iter().any(|trivia| trivia.is_comment()) {
                        self.append_leading_trivia(node, indent_level);
                    }
                    self.append_display(token);
                }
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
                // A postfix `[`/`(` can follow a comment on the previous line.
                mq_lang::TokenKind::LBracket | mq_lang::TokenKind::LParen
                    if node.leading_trivia.iter().any(|trivia| trivia.is_comment()) =>
                {
                    // The caller indents a child that starts a line; undo it to keep the expression's indent.
                    let indent_level = indent_level.saturating_sub(1);
                    self.append_leading_trivia(node, indent_level);
                    if self.output.ends_with('\n') {
                        self.append_indent(indent_level);
                    }
                    self.append_display(token);
                }
                _ => self.append_display(token),
            }
        }
    }

    #[inline(always)]
    fn append_display(&mut self, value: &impl std::fmt::Display) {
        let _ = write!(self.output, "{value}");
    }

    /// Appends a comment, keeping the whitespace after `#` so aligned comments stay aligned.
    fn append_comment(&mut self, trivia: &mq_lang::CstTrivia) {
        let mq_lang::CstTrivia::Comment(token) = trivia else {
            return;
        };
        let mq_lang::TokenKind::Comment(text) = &token.kind else {
            self.append_display(trivia);
            return;
        };
        let text = text.trim_end();
        if text.starts_with(char::is_whitespace) {
            let _ = write!(self.output, "#{text}");
        } else {
            let _ = write!(self.output, "# {text}");
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
    /// Indent level of the last line written to the output.
    pub fn current_line_indent(&self) -> usize {
        // Find the last newline position
        let start = self.output.rfind('\n').map_or(0, |pos| pos + 1);
        let last_line = &self.output[start..];
        last_line.chars().take_while(|c| *c == ' ').count() / self.config.indent_width
    }

    /// `current_line_indent` plus one level for a leading `| `.
    fn current_line_content_indent(&self) -> usize {
        let indent = self.current_line_indent();
        let start = self.output.rfind('\n').map_or(0, |pos| pos + 1);
        if self.output[start..].trim_start().starts_with('|') {
            indent + 1
        } else {
            indent
        }
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
    /// Whether the last output line ends with a pipe.
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

    /// Writes a string literal, keeping escape sequences as written in the source.
    fn append_string_literal(&mut self, token: &mq_lang::Token, value: &str) {
        self.output.push('"');
        match self.raw_literal(token, "\"") {
            Some(raw) => Self::push_escaping_controls(&mut self.output, &raw, false),
            None => self.output.push_str(&Self::escape_string(value)),
        }
        self.output.push('"');
    }

    /// Returns the source text between the quotes of a literal token.
    fn raw_literal(&self, token: &mq_lang::Token, prefix: &str) -> Option<String> {
        let raw = self.source.as_ref()?.slice(&token.range)?;
        raw.strip_prefix(prefix)?.strip_suffix('"').map(str::to_string)
    }

    /// Escapes raw control characters, leaving existing escape sequences untouched.
    /// Interpolated strings may span lines, so `keep_newlines` leaves raw newlines as is.
    fn push_escaping_controls(output: &mut String, raw: &str, keep_newlines: bool) {
        for ch in raw.chars() {
            match ch {
                '\n' if keep_newlines => output.push('\n'),
                '\n' => output.push_str("\\n"),
                '\t' => output.push_str("\\t"),
                '\r' => output.push_str("\\r"),
                c if c.is_control() => {
                    let code = c as u32;
                    if code <= 0xFF {
                        let _ = write!(output, "\\x{:02x}", code);
                    } else {
                        let _ = write!(output, "\\u{{{:04x}}}", code);
                    }
                }
                c => output.push(c),
            }
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
mod tests;
