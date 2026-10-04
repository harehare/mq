//! `.mqc` CODE section codec. Instructions use fixed wire IDs; decoded code is verified.
mod tags;
use super::MqcError;
use super::wire::{Reader, Writer};
use crate::ast::TokenId;
use crate::runtime::builtin;
use crate::runtime::runtime_value::{ResumeBuiltin, RuntimeValue};
use crate::selector::{AttrKind, Selector};
use crate::tarn::bytecode::{
    BinaryOp, Chunk, NodeSelectorKind, OpCode, ParamBinding, ParamShape, StaticExactCallTarget, TryCatchInfo,
    UpvalueSource, verify_chunks,
};
use crate::tarn::compiler::CompiledProgram;
use crate::tarn::split_program::SplitProgram;
use crate::{DictMap, Ident, Shared, TokenArena};
use rustc_hash::FxHashMap;

const MAX_CONSTANT_DEPTH: usize = 64;
/// Caps up-front allocation, since counts come from untrusted input.
pub(super) const MAX_PREALLOCATED: usize = 1 << 12;

/// CODE payload plus data for the other sections.
pub(crate) struct EncodedCode {
    pub(crate) payload: Vec<u8>,
    /// Indexed by span ID.
    pub(crate) tokens: Vec<TokenId>,
    pub(crate) external_globals: Vec<Ident>,
    pub(crate) builtins: Vec<Ident>,
}

#[derive(Default)]
struct Tables {
    idents: Vec<Ident>,
    ident_indexes: FxHashMap<Ident, u32>,
    tokens: Vec<TokenId>,
    token_indexes: FxHashMap<TokenId, u32>,
    external_globals: Vec<Ident>,
    builtins: Vec<Ident>,
}

impl Tables {
    fn ident(&mut self, writer: &mut Writer, ident: Ident) {
        let next = self.idents.len() as u32;
        let index = *self.ident_indexes.entry(ident).or_insert_with(|| {
            self.idents.push(ident);
            next
        });
        writer.var_u32(index);
    }

    fn token(&mut self, writer: &mut Writer, token_id: TokenId) {
        let next = self.tokens.len() as u32;
        let index = *self.token_indexes.entry(token_id).or_insert_with(|| {
            self.tokens.push(token_id);
            next
        });
        writer.var_u32(index);
    }

    fn note_external_global(&mut self, ident: Ident) {
        if !self.external_globals.contains(&ident) {
            self.external_globals.push(ident);
        }
    }

    fn note_builtin(&mut self, ident: Ident) {
        if builtin::get_builtin_functions(&ident).is_some() && !self.builtins.contains(&ident) {
            self.builtins.push(ident);
        }
    }
}

pub(crate) fn encode(program: &SplitProgram) -> Result<EncodedCode, MqcError> {
    let mut tables = Tables::default();
    let mut body = Writer::default();

    body.len(program.let_names.len())?;
    for name in &program.let_names {
        tables.ident(&mut body, *name);
    }
    body.bool(program.after.is_some());
    encode_program(&mut body, &mut tables, &program.program)?;
    if let Some(after) = &program.after {
        encode_program(&mut body, &mut tables, after)?;
    }

    // Identifier entries and the body share one decode budget.
    let mut payload = Writer::with_remaining_items(body.remaining_items());
    payload.len(tables.idents.len())?;
    for ident in &tables.idents {
        payload.str(&ident.as_str())?;
    }
    payload.raw(&body.into_bytes());

    Ok(EncodedCode {
        payload: payload.into_bytes(),
        tokens: tables.tokens,
        external_globals: tables.external_globals,
        builtins: tables.builtins,
    })
}

/// `tokens` maps span IDs to tokens in `token_arena`.
pub(crate) fn decode(payload: &[u8], tokens: &[TokenId], token_arena: TokenArena) -> Result<SplitProgram, MqcError> {
    let mut reader = Reader::new(payload);
    let ident_count = reader.len(1)?;
    let idents = (0..ident_count)
        .map(|_| reader.string().map(|name| Ident::new(&name)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut decoder = Decoder { reader, idents, tokens };

    let let_name_count = decoder.reader.len(1)?;
    let let_names = (0..let_name_count)
        .map(|_| decoder.ident())
        .collect::<Result<Vec<_>, _>>()?;
    let has_after = decoder.reader.bool()?;
    let program = decoder.program(Shared::clone(&token_arena))?;
    let after = if has_after {
        Some(decoder.program(token_arena)?)
    } else {
        None
    };
    decoder.reader.finish("the CODE section")?;
    Ok(SplitProgram::new(program, after, let_names))
}

fn encode_program(writer: &mut Writer, tables: &mut Tables, program: &CompiledProgram) -> Result<(), MqcError> {
    writer.len(program.chunks.len())?;
    for chunk in program.chunks.iter() {
        encode_chunk(writer, tables, chunk)?;
    }
    Ok(())
}

fn encode_chunk(writer: &mut Writer, tables: &mut Tables, chunk: &Chunk) -> Result<(), MqcError> {
    match chunk.function_name {
        Some(name) => {
            writer.bool(true);
            tables.ident(writer, name);
        }
        None => writer.bool(false),
    }
    writer.bool(chunk.is_generator);

    writer.len(chunk.local_names.len())?;
    for (name, mutable) in chunk.local_names.iter().zip(&chunk.local_mutable) {
        tables.ident(writer, *name);
        writer.bool(*mutable);
    }
    writer.len(chunk.upvalue_names.len())?;
    for name in &chunk.upvalue_names {
        tables.ident(writer, *name);
    }

    encode_param_shape(writer, &chunk.param_shape)?;

    writer.len(chunk.constants.len())?;
    for constant in &chunk.constants {
        encode_constant(writer, tables, constant, 0)?;
    }
    writer.len(chunk.static_closures.len())?;
    for closure in &chunk.static_closures {
        writer.var_u16(closure.chunk_index);
    }

    writer.len(chunk.code.len())?;
    for op in &chunk.code {
        encode_op(writer, tables, op)?;
    }
    writer.len(chunk.lines.len())?;
    let mut previous_pc = 0;
    for line in &chunk.lines {
        writer.len(line.pc_start - previous_pc)?;
        previous_pc = line.pc_start;
        tables.token(writer, line.token_id);
    }
    Ok(())
}

fn encode_param_shape(writer: &mut Writer, shape: &ParamShape) -> Result<(), MqcError> {
    writer.len(shape.bindings.len())?;
    for binding in &shape.bindings {
        match binding {
            ParamBinding::Required(slot) => {
                writer.u8(tags::parameter::REQUIRED);
                writer.var_u16(*slot);
            }
            ParamBinding::Optional(slot, default_chunk, sources) => {
                writer.u8(tags::parameter::OPTIONAL);
                writer.var_u16(*slot);
                writer.var_u16(*default_chunk);
                encode_upvalue_sources(writer, sources)?;
            }
            ParamBinding::Variadic(slot) => {
                writer.u8(tags::parameter::VARIADIC);
                writer.var_u16(*slot);
            }
        }
    }
    Ok(())
}

fn encode_upvalue_sources(writer: &mut Writer, sources: &[UpvalueSource]) -> Result<(), MqcError> {
    writer.len(sources.len())?;
    for source in sources {
        match source {
            UpvalueSource::Local(slot) => {
                writer.u8(tags::capture::LOCAL);
                writer.var_u16(*slot);
            }
            UpvalueSource::Upvalue(index) => {
                writer.u8(tags::capture::UPVALUE);
                writer.var_u16(*index);
            }
        }
    }
    Ok(())
}

fn encode_constant(
    writer: &mut Writer,
    tables: &mut Tables,
    value: &RuntimeValue,
    depth: usize,
) -> Result<(), MqcError> {
    if depth > MAX_CONSTANT_DEPTH {
        return Err(MqcError::UnsupportedValue(format!(
            "a constant nested deeper than {MAX_CONSTANT_DEPTH} levels"
        )));
    }
    match value {
        RuntimeValue::None => writer.u8(tags::constant::NONE),
        RuntimeValue::Number(number) => {
            writer.u8(tags::constant::NUMBER);
            writer.f64(number.value());
        }
        RuntimeValue::Boolean(value) => {
            writer.u8(tags::constant::BOOLEAN);
            writer.bool(*value);
        }
        RuntimeValue::String(value) => {
            writer.u8(tags::constant::STRING);
            writer.str(value)?;
        }
        RuntimeValue::Symbol(ident) => {
            writer.u8(tags::constant::SYMBOL);
            tables.ident(writer, *ident);
        }
        RuntimeValue::Bytes(bytes) => {
            writer.u8(tags::constant::BYTES);
            writer.bytes(bytes)?;
        }
        RuntimeValue::Array(values) => {
            writer.u8(tags::constant::ARRAY);
            writer.len(values.len())?;
            for value in values.iter() {
                encode_constant(writer, tables, value, depth + 1)?;
            }
        }
        RuntimeValue::Dict(map) => {
            writer.u8(tags::constant::DICT);
            writer.len(map.len())?;
            for (key, value) in map.iter() {
                tables.ident(writer, *key);
                encode_constant(writer, tables, value, depth + 1)?;
            }
        }
        RuntimeValue::NativeFunction(ident) => {
            writer.u8(tags::constant::NATIVE_FUNCTION);
            tables.note_builtin(*ident);
            tables.ident(writer, *ident);
        }
        RuntimeValue::CoroutineBuiltin(builtin) => {
            writer.u8(tags::constant::COROUTINE_BUILTIN);
            writer.u8(match builtin {
                ResumeBuiltin::Next => tags::coroutine::NEXT,
                ResumeBuiltin::Send => tags::coroutine::SEND,
            });
        }
        RuntimeValue::Markdown(..) => return Err(unsupported_constant("a Markdown node")),
        RuntimeValue::Closure(_) => return Err(unsupported_constant("a function")),
        RuntimeValue::Coroutine(_) | RuntimeValue::WeakCoroutine(_) => {
            return Err(unsupported_constant("a coroutine"));
        }
        #[cfg(any(feature = "file-io", feature = "http"))]
        RuntimeValue::ReaderHandle(_) => return Err(unsupported_constant("a file handle")),
    }
    Ok(())
}

fn unsupported_constant(what: &str) -> MqcError {
    MqcError::UnsupportedValue(format!(
        "{what} (a module-level `let` evaluates to it at compile time; compute it inside a function instead)"
    ))
}

fn binary_op_id(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Add => tags::binary::ADD,
        BinaryOp::Sub => tags::binary::SUB,
        BinaryOp::Mul => tags::binary::MUL,
        BinaryOp::Div => tags::binary::DIV,
        BinaryOp::Mod => tags::binary::MOD,
        BinaryOp::Eq => tags::binary::EQ,
        BinaryOp::Ne => tags::binary::NE,
        BinaryOp::Lt => tags::binary::LT,
        BinaryOp::Le => tags::binary::LE,
        BinaryOp::Gt => tags::binary::GT,
        BinaryOp::Ge => tags::binary::GE,
    }
}

fn binary_op_from_id(id: u8) -> Result<BinaryOp, MqcError> {
    Ok(match id {
        tags::binary::ADD => BinaryOp::Add,
        tags::binary::SUB => BinaryOp::Sub,
        tags::binary::MUL => BinaryOp::Mul,
        tags::binary::DIV => BinaryOp::Div,
        tags::binary::MOD => BinaryOp::Mod,
        tags::binary::EQ => BinaryOp::Eq,
        tags::binary::NE => BinaryOp::Ne,
        tags::binary::LT => BinaryOp::Lt,
        tags::binary::LE => BinaryOp::Le,
        tags::binary::GT => BinaryOp::Gt,
        tags::binary::GE => BinaryOp::Ge,
        other => return Err(invalid(format!("unknown binary operator {other}"))),
    })
}

fn attr_kind_id(kind: &AttrKind) -> u8 {
    match kind {
        AttrKind::Value => tags::attribute::VALUE,
        AttrKind::Values => tags::attribute::VALUES,
        AttrKind::Children => tags::attribute::CHILDREN,
        AttrKind::Lang => tags::attribute::LANG,
        AttrKind::Meta => tags::attribute::META,
        AttrKind::Fence => tags::attribute::FENCE,
        AttrKind::Url => tags::attribute::URL,
        AttrKind::Alt => tags::attribute::ALT,
        AttrKind::Title => tags::attribute::TITLE,
        AttrKind::Ident => tags::attribute::IDENT,
        AttrKind::Label => tags::attribute::LABEL,
        AttrKind::Depth => tags::attribute::DEPTH,
        AttrKind::Level => tags::attribute::LEVEL,
        AttrKind::Index => tags::attribute::INDEX,
        AttrKind::Ordered => tags::attribute::ORDERED,
        AttrKind::Checked => tags::attribute::CHECKED,
        AttrKind::Column => tags::attribute::COLUMN,
        AttrKind::Row => tags::attribute::ROW,
        AttrKind::Align => tags::attribute::ALIGN,
        AttrKind::Name => tags::attribute::NAME,
        AttrKind::Kind => tags::attribute::KIND,
        AttrKind::Line => tags::attribute::LINE,
        AttrKind::EndLine => tags::attribute::END_LINE,
    }
}

fn attr_kind_from_id(id: u8) -> Result<AttrKind, MqcError> {
    Ok(match id {
        tags::attribute::VALUE => AttrKind::Value,
        tags::attribute::VALUES => AttrKind::Values,
        tags::attribute::CHILDREN => AttrKind::Children,
        tags::attribute::LANG => AttrKind::Lang,
        tags::attribute::META => AttrKind::Meta,
        tags::attribute::FENCE => AttrKind::Fence,
        tags::attribute::URL => AttrKind::Url,
        tags::attribute::ALT => AttrKind::Alt,
        tags::attribute::TITLE => AttrKind::Title,
        tags::attribute::IDENT => AttrKind::Ident,
        tags::attribute::LABEL => AttrKind::Label,
        tags::attribute::DEPTH => AttrKind::Depth,
        tags::attribute::LEVEL => AttrKind::Level,
        tags::attribute::INDEX => AttrKind::Index,
        tags::attribute::ORDERED => AttrKind::Ordered,
        tags::attribute::CHECKED => AttrKind::Checked,
        tags::attribute::COLUMN => AttrKind::Column,
        tags::attribute::ROW => AttrKind::Row,
        tags::attribute::ALIGN => AttrKind::Align,
        tags::attribute::NAME => AttrKind::Name,
        tags::attribute::KIND => AttrKind::Kind,
        tags::attribute::LINE => AttrKind::Line,
        tags::attribute::END_LINE => AttrKind::EndLine,
        other => return Err(invalid(format!("unknown attribute selector {other}"))),
    })
}

fn encode_optional_index(writer: &mut Writer, value: Option<usize>) -> Result<(), MqcError> {
    match value {
        Some(value) => {
            writer.bool(true);
            let value = u64::try_from(value).map_err(|_| MqcError::UnsupportedValue("a selector index".into()))?;
            writer.var(value);
        }
        None => writer.bool(false),
    }
    Ok(())
}

fn encode_selector(writer: &mut Writer, tables: &mut Tables, selector: &Selector) -> Result<(), MqcError> {
    match selector {
        Selector::Blockquote => writer.u8(tags::selector::BLOCKQUOTE),
        Selector::Footnote => writer.u8(tags::selector::FOOTNOTE),
        Selector::List(index, ordered) => {
            writer.u8(tags::selector::LIST);
            encode_optional_index(writer, *index)?;
            match ordered {
                Some(ordered) => {
                    writer.bool(true);
                    writer.bool(*ordered);
                }
                None => writer.bool(false),
            }
        }
        Selector::Toml => writer.u8(tags::selector::TOML),
        Selector::Yaml => writer.u8(tags::selector::YAML),
        Selector::Break => writer.u8(tags::selector::BREAK),
        Selector::InlineCode => writer.u8(tags::selector::INLINE_CODE),
        Selector::InlineMath => writer.u8(tags::selector::INLINE_MATH),
        Selector::Delete => writer.u8(tags::selector::DELETE),
        Selector::Emphasis => writer.u8(tags::selector::EMPHASIS),
        Selector::FootnoteRef => writer.u8(tags::selector::FOOTNOTE_REF),
        Selector::Html => writer.u8(tags::selector::HTML),
        Selector::Image => writer.u8(tags::selector::IMAGE),
        Selector::ImageRef => writer.u8(tags::selector::IMAGE_REF),
        Selector::MdxJsxTextElement => writer.u8(tags::selector::MDX_JSX_TEXT_ELEMENT),
        Selector::Link => writer.u8(tags::selector::LINK),
        Selector::LinkRef => writer.u8(tags::selector::LINK_REF),
        Selector::WikiLink => writer.u8(tags::selector::WIKI_LINK),
        Selector::Callout => writer.u8(tags::selector::CALLOUT),
        Selector::Embed => writer.u8(tags::selector::EMBED),
        Selector::Strong => writer.u8(tags::selector::STRONG),
        Selector::Code => writer.u8(tags::selector::CODE),
        Selector::Math => writer.u8(tags::selector::MATH),
        Selector::Heading(level) => {
            writer.u8(tags::selector::HEADING);
            match level {
                Some(level) => {
                    writer.bool(true);
                    writer.u8(*level);
                }
                None => writer.bool(false),
            }
        }
        Selector::Table(row, column) => {
            writer.u8(tags::selector::TABLE);
            encode_optional_index(writer, *row)?;
            encode_optional_index(writer, *column)?;
        }
        Selector::TableAlign => writer.u8(tags::selector::TABLE_ALIGN),
        Selector::Text => writer.u8(tags::selector::TEXT),
        Selector::HorizontalRule => writer.u8(tags::selector::HORIZONTAL_RULE),
        Selector::Definition => writer.u8(tags::selector::DEFINITION),
        Selector::MdxFlowExpression => writer.u8(tags::selector::MDX_FLOW_EXPRESSION),
        Selector::MdxTextExpression => writer.u8(tags::selector::MDX_TEXT_EXPRESSION),
        Selector::MdxJsEsm => writer.u8(tags::selector::MDX_JS_ESM),
        Selector::MdxJsxFlowElement => writer.u8(tags::selector::MDX_JSX_FLOW_ELEMENT),
        Selector::Recursive => writer.u8(tags::selector::RECURSIVE),
        Selector::Task => writer.u8(tags::selector::TASK),
        Selector::Todo => writer.u8(tags::selector::TODO),
        Selector::Done => writer.u8(tags::selector::DONE),
        Selector::Attr(kind) => {
            writer.u8(tags::selector::ATTR);
            writer.u8(attr_kind_id(kind));
        }
        Selector::Property(ident) => {
            writer.u8(tags::selector::PROPERTY);
            tables.ident(writer, *ident);
        }
    }
    Ok(())
}

fn encode_op(writer: &mut Writer, tables: &mut Tables, op: &OpCode) -> Result<(), MqcError> {
    match op {
        #[cfg(feature = "debugger")]
        OpCode::StmtBoundary(_) | OpCode::SyncCallNode(_) | OpCode::Breakpoint(_) => {
            return Err(MqcError::InvalidBytecode(
                "debugger instrumentation cannot be saved".to_string(),
            ));
        }
        OpCode::Const(index) => {
            writer.u8(tags::opcode::CONST);
            writer.var_u16(*index);
        }
        OpCode::PushNone => writer.u8(tags::opcode::PUSH_NONE),
        OpCode::GetLocal(slot) => {
            writer.u8(tags::opcode::GET_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::SetLocal(slot) => {
            writer.u8(tags::opcode::SET_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::SetLocalAndCopy { source, destination } => {
            writer.u8(tags::opcode::SET_LOCAL_AND_COPY);
            writer.var_u16(*source);
            writer.var_u16(*destination);
        }
        OpCode::SetLocalAndCopyAndJump {
            source,
            destination,
            offset,
        } => {
            writer.u8(tags::opcode::SET_LOCAL_AND_COPY_AND_JUMP);
            writer.var_u16(*source);
            writer.var_u16(*destination);
            writer.var_i32(*offset);
        }
        OpCode::SetLocalConst { local, constant } => {
            writer.u8(tags::opcode::SET_LOCAL_CONST);
            writer.var_u16(*local);
            writer.var_u16(*constant);
        }
        OpCode::TeeLocal(slot) => {
            writer.u8(tags::opcode::TEE_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::CopyLocal { source, destination } => {
            writer.u8(tags::opcode::COPY_LOCAL);
            writer.var_u16(*source);
            writer.var_u16(*destination);
        }
        OpCode::GetUpvalue(index) => {
            writer.u8(tags::opcode::GET_UPVALUE);
            writer.var_u16(*index);
        }
        OpCode::SetUpvalue(index) => {
            writer.u8(tags::opcode::SET_UPVALUE);
            writer.var_u16(*index);
        }
        OpCode::MakeClosure(payload) => {
            writer.u8(tags::opcode::MAKE_CLOSURE);
            writer.var_u16(payload.0);
            encode_upvalue_sources(writer, &payload.1)?;
        }
        OpCode::MakeStaticClosure(index) => {
            writer.u8(tags::opcode::MAKE_STATIC_CLOSURE);
            writer.var_u16(*index);
        }
        OpCode::Pop => writer.u8(tags::opcode::POP),
        OpCode::Dup => writer.u8(tags::opcode::DUP),
        OpCode::Jump(offset) => {
            writer.u8(tags::opcode::JUMP);
            writer.var_i32(*offset);
        }
        OpCode::JumpIfFalse(offset) => {
            writer.u8(tags::opcode::JUMP_IF_FALSE);
            writer.var_i32(*offset);
        }
        OpCode::JumpIfTrue(offset) => {
            writer.u8(tags::opcode::JUMP_IF_TRUE);
            writer.var_i32(*offset);
        }
        OpCode::Add => writer.u8(tags::opcode::ADD),
        OpCode::Sub => writer.u8(tags::opcode::SUB),
        OpCode::Mul => writer.u8(tags::opcode::MUL),
        OpCode::Div => writer.u8(tags::opcode::DIV),
        OpCode::Mod => writer.u8(tags::opcode::MOD),
        OpCode::Eq => writer.u8(tags::opcode::EQ),
        OpCode::Ne => writer.u8(tags::opcode::NE),
        OpCode::Lt => writer.u8(tags::opcode::LT),
        OpCode::Le => writer.u8(tags::opcode::LE),
        OpCode::Gt => writer.u8(tags::opcode::GT),
        OpCode::Ge => writer.u8(tags::opcode::GE),
        OpCode::BinaryLocalLocal { op, left, right } => {
            writer.u8(tags::opcode::BINARY_LOCAL_LOCAL);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*left);
            writer.var_u16(*right);
        }
        OpCode::BinaryLocalConst { op, local, constant } => {
            writer.u8(tags::opcode::BINARY_LOCAL_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_u16(*constant);
        }
        OpCode::BinaryLocalNumberConst { op, local, constant } => {
            writer.u8(tags::opcode::BINARY_LOCAL_NUMBER_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_i32(*constant);
        }
        OpCode::UpdateLocalConst { op, local, constant } => {
            writer.u8(tags::opcode::UPDATE_LOCAL_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_u16(*constant);
        }
        OpCode::UpdateLocalNumberConst { op, local, constant } => {
            writer.u8(tags::opcode::UPDATE_LOCAL_NUMBER_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_i32(*constant);
        }
        OpCode::UpdateLocalLocal { op, local, value } => {
            writer.u8(tags::opcode::UPDATE_LOCAL_LOCAL);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_u16(*value);
        }
        OpCode::JumpIfFalseLocalLocal {
            op,
            left,
            right,
            offset,
        } => {
            writer.u8(tags::opcode::JUMP_IF_FALSE_LOCAL_LOCAL);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*left);
            writer.var_u16(*right);
            writer.var_i32(*offset);
        }
        OpCode::JumpIfFalseLocalConst {
            op,
            local,
            constant,
            offset,
        } => {
            writer.u8(tags::opcode::JUMP_IF_FALSE_LOCAL_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_u16(*constant);
            writer.var_i32(*offset);
        }
        OpCode::JumpIfFalseLocalNumberConst {
            op,
            local,
            constant,
            offset,
        } => {
            writer.u8(tags::opcode::JUMP_IF_FALSE_LOCAL_NUMBER_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_i32(*constant);
            writer.var_i32(*offset);
        }
        OpCode::Neg => writer.u8(tags::opcode::NEG),
        OpCode::Not => writer.u8(tags::opcode::NOT),
        OpCode::ArrayNew => writer.u8(tags::opcode::ARRAY_NEW),
        OpCode::ArrayNewWithCapacityLocal(slot) => {
            writer.u8(tags::opcode::ARRAY_NEW_WITH_CAPACITY_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::ArrayPush => writer.u8(tags::opcode::ARRAY_PUSH),
        OpCode::ArraySpread => writer.u8(tags::opcode::ARRAY_SPREAD),
        OpCode::DictNew => writer.u8(tags::opcode::DICT_NEW),
        OpCode::DictInsert => writer.u8(tags::opcode::DICT_INSERT),
        OpCode::DictSpread => writer.u8(tags::opcode::DICT_SPREAD),
        OpCode::ToForeachIterable => writer.u8(tags::opcode::TO_FOREACH_ITERABLE),
        OpCode::ArrayLen => writer.u8(tags::opcode::ARRAY_LEN),
        OpCode::ArrayGetAt => writer.u8(tags::opcode::ARRAY_GET_AT),
        OpCode::ArrayLenLocal(slot) => {
            writer.u8(tags::opcode::ARRAY_LEN_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::ArrayGetLocalAt { array_slot, index_slot } => {
            writer.u8(tags::opcode::ARRAY_GET_LOCAL_AT);
            writer.var_u16(*array_slot);
            writer.var_u16(*index_slot);
        }
        OpCode::ForeachNext {
            array_slot,
            index_slot,
            value_slot,
            exit_offset,
        } => {
            writer.u8(tags::opcode::FOREACH_NEXT);
            writer.var_u16(*array_slot);
            writer.var_u16(*index_slot);
            writer.var_u16(*value_slot);
            writer.var_i32(*exit_offset);
        }
        OpCode::ForeachCollect(slot) => {
            writer.u8(tags::opcode::FOREACH_COLLECT);
            writer.var_u16(*slot);
        }
        OpCode::ForeachCollectAndJump { slot, offset } => {
            writer.u8(tags::opcode::FOREACH_COLLECT_AND_JUMP);
            writer.var_u16(*slot);
            writer.var_i32(*offset);
        }
        OpCode::ForeachBinaryLocalNumberConstAndJump {
            op,
            local,
            constant,
            accumulator_slot,
            offset,
        } => {
            writer.u8(tags::opcode::FOREACH_BINARY_LOCAL_NUMBER_CONST_AND_JUMP);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_i32(*constant);
            writer.var_u16(*accumulator_slot);
            writer.var_i32(*offset);
        }
        OpCode::ArraySliceFrom => writer.u8(tags::opcode::ARRAY_SLICE_FROM),
        OpCode::DictGetLocalOrFail {
            subject_slot,
            key,
            value_slot,
        } => {
            writer.u8(tags::opcode::DICT_GET_LOCAL_OR_FAIL);
            writer.var_u16(*subject_slot);
            tables.ident(writer, *key);
            writer.var_u16(*value_slot);
        }
        OpCode::TypeCheck(ident) => {
            writer.u8(tags::opcode::TYPE_CHECK);
            tables.ident(writer, *ident);
        }
        OpCode::GetEnvVar(index) => {
            writer.u8(tags::opcode::GET_ENV_VAR);
            writer.var_u16(*index);
        }
        OpCode::GetExternalGlobal(ident) => {
            writer.u8(tags::opcode::GET_EXTERNAL_GLOBAL);
            tables.note_external_global(*ident);
            tables.ident(writer, *ident);
        }
        OpCode::InterpString(count) => {
            writer.u8(tags::opcode::INTERP_STRING);
            writer.var_u16(*count);
        }
        OpCode::SelectorMatch(selector) => {
            writer.u8(tags::opcode::SELECTOR_MATCH);
            encode_selector(writer, tables, selector)?;
        }
        OpCode::SelectorMatchKind(kind) => {
            writer.u8(tags::opcode::SELECTOR_MATCH_KIND);
            encode_selector(writer, tables, &kind.as_selector())?;
        }
        OpCode::SelectorMatchHeading(level) => {
            writer.u8(tags::opcode::SELECTOR_MATCH_HEADING);
            writer.u8(*level);
        }
        OpCode::SelectorMatchWithArgs(payload) => {
            writer.u8(tags::opcode::SELECTOR_MATCH_WITH_ARGS);
            encode_selector(writer, tables, &payload.0)?;
            writer.var_u16(payload.1);
        }
        OpCode::CallBuiltinLocal { builtin, local, .. } => {
            writer.u8(tags::opcode::CALL_BUILTIN_LOCAL);
            tables.note_builtin(*builtin);
            tables.ident(writer, *builtin);
            writer.var_u16(*local);
        }
        OpCode::CallNative { ident, argc, .. } | OpCode::CallBuiltin(ident, argc) => {
            writer.u8(tags::opcode::CALL_BUILTIN);
            tables.note_builtin(*ident);
            tables.ident(writer, *ident);
            writer.var_u16(*argc);
        }
        OpCode::CallStatic(target, argc) => {
            writer.u8(tags::opcode::CALL_STATIC);
            writer.var_u16(*target);
            writer.var_u16(*argc);
        }
        OpCode::CallStaticExact(target, argc) => {
            writer.u8(tags::opcode::CALL_STATIC_EXACT);
            writer.var_u16(*target);
            writer.var_u16(*argc);
        }
        OpCode::CallStaticExact0(target) => {
            writer.u8(tags::opcode::CALL_STATIC_EXACT0);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticExact1(target) => {
            writer.u8(tags::opcode::CALL_STATIC_EXACT1);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticExact2(target) => {
            writer.u8(tags::opcode::CALL_STATIC_EXACT2);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticImplicitSelf(target, argc) => {
            writer.u8(tags::opcode::CALL_STATIC_IMPLICIT_SELF);
            writer.var_u16(*target);
            writer.var_u16(*argc);
        }
        OpCode::CallSelf(argc) => {
            writer.u8(tags::opcode::CALL_SELF);
            writer.var_u16(*argc);
        }
        OpCode::CallSelfExact(argc) => {
            writer.u8(tags::opcode::CALL_SELF_EXACT);
            writer.var_u16(*argc);
        }
        OpCode::CallSelfExact0 => writer.u8(tags::opcode::CALL_SELF_EXACT0),
        OpCode::CallSelfExact1 => writer.u8(tags::opcode::CALL_SELF_EXACT1),
        OpCode::CallSelfExact2 => writer.u8(tags::opcode::CALL_SELF_EXACT2),
        OpCode::CallSelfImplicitSelf(argc) => {
            writer.u8(tags::opcode::CALL_SELF_IMPLICIT_SELF);
            writer.var_u16(*argc);
        }
        OpCode::CallLocal(slot, argc) => {
            writer.u8(tags::opcode::CALL_LOCAL);
            writer.var_u16(*slot);
            writer.var_u16(*argc);
        }
        OpCode::CallUpvalue(index, argc) => {
            writer.u8(tags::opcode::CALL_UPVALUE);
            writer.var_u16(*index);
            writer.var_u16(*argc);
        }
        OpCode::CallUpvalueLocal { index, local } => {
            writer.u8(tags::opcode::CALL_UPVALUE_LOCAL);
            writer.var_u16(*index);
            writer.var_u16(*local);
        }
        OpCode::CallValue(argc) => {
            writer.u8(tags::opcode::CALL_VALUE);
            writer.var_u16(*argc);
        }
        OpCode::MaybeAutoCall => writer.u8(tags::opcode::MAYBE_AUTO_CALL),
        OpCode::TryCatch(info) => {
            writer.u8(tags::opcode::TRY_CATCH);
            writer.bool(info.has_binder);
            encode_optional_u16(writer, info.break_acc_slot);
            encode_optional_u16(writer, info.break_completed_iteration_slot);
            encode_optional_i32(writer, info.break_offset);
            encode_optional_i32(writer, info.continue_offset);
        }
        OpCode::FlowBreak(has_value) => {
            writer.u8(tags::opcode::FLOW_BREAK);
            writer.bool(*has_value);
        }
        OpCode::FlowContinue => writer.u8(tags::opcode::FLOW_CONTINUE),
        OpCode::RaiseDestructuringFailed => writer.u8(tags::opcode::RAISE_DESTRUCTURING_FAILED),
        OpCode::ReturnLocal(slot) => {
            writer.u8(tags::opcode::RETURN_LOCAL);
            writer.var_u16(*slot);
        }
        OpCode::ReturnBinaryLocalLocal { op, left, right } => {
            writer.u8(tags::opcode::RETURN_BINARY_LOCAL_LOCAL);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*left);
            writer.var_u16(*right);
        }
        OpCode::ReturnBinaryLocalConst { op, local, constant } => {
            writer.u8(tags::opcode::RETURN_BINARY_LOCAL_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_u16(*constant);
        }
        OpCode::ReturnBinaryLocalNumberConst { op, local, constant } => {
            writer.u8(tags::opcode::RETURN_BINARY_LOCAL_NUMBER_CONST);
            writer.u8(binary_op_id(*op));
            writer.var_u16(*local);
            writer.var_i32(*constant);
        }
        OpCode::Return => writer.u8(tags::opcode::RETURN),
        OpCode::Yield => writer.u8(tags::opcode::YIELD),
        OpCode::Resume(argc) => {
            writer.u8(tags::opcode::RESUME);
            writer.u8(*argc);
        }
    }
    Ok(())
}

fn encode_static_target(writer: &mut Writer, target: &StaticExactCallTarget) {
    writer.var_u16(target.chunk_index);
    writer.var_u16(target.local_count);
}

fn encode_optional_u16(writer: &mut Writer, value: Option<u16>) {
    match value {
        Some(value) => {
            writer.bool(true);
            writer.var_u16(value);
        }
        None => writer.bool(false),
    }
}

fn encode_optional_i32(writer: &mut Writer, value: Option<i32>) {
    match value {
        Some(value) => {
            writer.bool(true);
            writer.var_i32(value);
        }
        None => writer.bool(false),
    }
}

fn invalid(message: String) -> MqcError {
    MqcError::InvalidBytecode(message)
}

fn ensure_vm_indexed_count(count: usize, what: &str) -> Result<(), MqcError> {
    if count > usize::from(u16::MAX) + 1 {
        return Err(invalid(format!("{count} {what} exceed the VM index limit")));
    }
    Ok(())
}

struct Decoder<'a> {
    reader: Reader<'a>,
    idents: Vec<Ident>,
    tokens: &'a [TokenId],
}

impl Decoder<'_> {
    fn ident(&mut self) -> Result<Ident, MqcError> {
        let index = self.reader.var_u32()? as usize;
        self.idents
            .get(index)
            .copied()
            .ok_or_else(|| invalid(format!("identifier {index} is out of bounds")))
    }

    fn token(&mut self) -> Result<TokenId, MqcError> {
        let index = self.reader.var_u32()? as usize;
        self.tokens
            .get(index)
            .copied()
            .ok_or_else(|| invalid(format!("source span {index} is out of bounds")))
    }

    fn program(&mut self, token_arena: TokenArena) -> Result<CompiledProgram, MqcError> {
        let chunk_count = self.reader.len(1)?;
        if chunk_count == 0 {
            return Err(invalid("a program has no chunks".to_string()));
        }
        ensure_vm_indexed_count(chunk_count, "chunks")?;
        let mut chunks = (0..chunk_count).map(|_| self.chunk()).collect::<Result<Vec<_>, _>>()?;
        for chunk in &mut chunks {
            chunk.refresh_captured_local_slots();
        }
        verify_chunks(&chunks).map_err(|error| invalid(error.to_string()))?;
        Ok(CompiledProgram {
            chunks: Shared::new(chunks),
            token_arena,
            #[cfg(feature = "debugger")]
            debug_sources: Vec::new(),
        })
    }

    fn chunk(&mut self) -> Result<Chunk, MqcError> {
        let function_name = if self.reader.bool()? { Some(self.ident()?) } else { None };
        let is_generator = self.reader.bool()?;

        let local_count = self.reader.len(2)?;
        let local_count_u16 =
            u16::try_from(local_count).map_err(|_| invalid(format!("{local_count} locals exceed the VM limit")))?;
        let mut local_names = Vec::with_capacity(local_count.min(MAX_PREALLOCATED));
        let mut local_mutable = Vec::with_capacity(local_count.min(MAX_PREALLOCATED));
        for _ in 0..local_count {
            local_names.push(self.ident()?);
            local_mutable.push(self.reader.bool()?);
        }
        let upvalue_count = self.reader.len(1)?;
        ensure_vm_indexed_count(upvalue_count, "upvalues")?;
        let upvalue_names = (0..upvalue_count)
            .map(|_| self.ident())
            .collect::<Result<Vec<_>, _>>()?;

        let param_shape = self.param_shape()?;

        let constant_count = self.reader.len(1)?;
        ensure_vm_indexed_count(constant_count, "constants")?;
        let constants = (0..constant_count)
            .map(|_| self.constant(0))
            .collect::<Result<Vec<_>, _>>()?;
        let static_closure_count = self.reader.len(1)?;
        ensure_vm_indexed_count(static_closure_count, "static closures")?;
        let mut chunk = Chunk::default();
        for _ in 0..static_closure_count {
            let target = self.reader.var_u16()?;
            chunk.push_static_closure(target);
        }

        let op_count = self.reader.len(1)?;
        let code = (0..op_count).map(|_| self.op()).collect::<Result<Vec<_>, _>>()?;
        let line_count = self.reader.len(2)?;
        if line_count > code.len() {
            return Err(invalid("more source positions than instructions".to_string()));
        }
        let mut lines = Vec::with_capacity(line_count.min(MAX_PREALLOCATED));
        let mut previous_pc = 0usize;
        for _ in 0..line_count {
            let pc_start = previous_pc
                .checked_add(self.reader.var_u32()? as usize)
                .ok_or_else(|| invalid("source position is out of range".to_string()))?;
            previous_pc = pc_start;
            let token_id = self.token()?;
            if pc_start >= code.len()
                || lines
                    .last()
                    .is_some_and(|last: &crate::tarn::bytecode::LineEntry| last.pc_start >= pc_start)
            {
                return Err(invalid("source positions are out of order".to_string()));
            }
            lines.push(crate::tarn::bytecode::LineEntry { pc_start, token_id });
        }

        chunk.code = code;
        chunk.constants = constants;
        chunk.local_count = local_count_u16;
        chunk.local_names = local_names;
        chunk.local_mutable = local_mutable;
        chunk.upvalue_names = upvalue_names;
        chunk.lines = lines;
        chunk.param_shape = param_shape;
        chunk.function_name = function_name;
        chunk.is_generator = is_generator;
        Ok(chunk)
    }

    /// Required, then optional, then at most one variadic.
    fn param_shape(&mut self) -> Result<ParamShape, MqcError> {
        let count = self.reader.len(2)?;
        let mut bindings = Vec::with_capacity(count.min(MAX_PREALLOCATED));
        let mut required = 0;
        let mut has_variadic = false;
        for _ in 0..count {
            if has_variadic {
                return Err(invalid("a parameter follows a variadic parameter".to_string()));
            }
            let binding = match self.reader.u8()? {
                tags::parameter::REQUIRED => {
                    if bindings.len() != required {
                        return Err(invalid("a required parameter follows an optional one".to_string()));
                    }
                    required += 1;
                    ParamBinding::Required(self.reader.var_u16()?)
                }
                tags::parameter::OPTIONAL => {
                    let slot = self.reader.var_u16()?;
                    let default_chunk = self.reader.var_u16()?;
                    ParamBinding::Optional(slot, default_chunk, self.upvalue_sources()?)
                }
                tags::parameter::VARIADIC => {
                    has_variadic = true;
                    ParamBinding::Variadic(self.reader.var_u16()?)
                }
                other => return Err(invalid(format!("unknown parameter kind {other}"))),
            };
            bindings.push(binding);
        }
        Ok(ParamShape {
            bindings,
            required,
            has_variadic,
        })
    }

    fn upvalue_sources(&mut self) -> Result<Vec<UpvalueSource>, MqcError> {
        let count = self.reader.len(2)?;
        (0..count)
            .map(|_| match self.reader.u8()? {
                tags::capture::LOCAL => Ok(UpvalueSource::Local(self.reader.var_u16()?)),
                tags::capture::UPVALUE => Ok(UpvalueSource::Upvalue(self.reader.var_u16()?)),
                other => Err(invalid(format!("unknown capture kind {other}"))),
            })
            .collect()
    }

    fn constant(&mut self, depth: usize) -> Result<RuntimeValue, MqcError> {
        if depth > MAX_CONSTANT_DEPTH {
            return Err(invalid(format!(
                "a constant is nested deeper than {MAX_CONSTANT_DEPTH} levels"
            )));
        }
        Ok(match self.reader.u8()? {
            tags::constant::NONE => RuntimeValue::None,
            tags::constant::NUMBER => RuntimeValue::Number(self.reader.f64()?.into()),
            tags::constant::BOOLEAN => RuntimeValue::Boolean(self.reader.bool()?),
            tags::constant::STRING => RuntimeValue::String(Shared::new(self.reader.string()?)),
            tags::constant::SYMBOL => RuntimeValue::Symbol(self.ident()?),
            tags::constant::BYTES => RuntimeValue::Bytes(Shared::new(self.reader.bytes()?.to_vec())),
            tags::constant::ARRAY => {
                let count = self.reader.len(1)?;
                let values = (0..count)
                    .map(|_| self.constant(depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                RuntimeValue::Array(Shared::new(values))
            }
            tags::constant::DICT => {
                let count = self.reader.len(2)?;
                let mut map = DictMap::default();
                for _ in 0..count {
                    let key = self.ident()?;
                    let value = self.constant(depth + 1)?;
                    map.insert(key, value);
                }
                RuntimeValue::Dict(Shared::new(map))
            }
            tags::constant::NATIVE_FUNCTION => RuntimeValue::NativeFunction(self.ident()?),
            tags::constant::COROUTINE_BUILTIN => RuntimeValue::CoroutineBuiltin(match self.reader.u8()? {
                tags::coroutine::NEXT => ResumeBuiltin::Next,
                tags::coroutine::SEND => ResumeBuiltin::Send,
                other => return Err(invalid(format!("unknown coroutine builtin {other}"))),
            }),
            other => return Err(invalid(format!("unknown constant tag {other}"))),
        })
    }

    fn optional_index(&mut self) -> Result<Option<usize>, MqcError> {
        if !self.reader.bool()? {
            return Ok(None);
        }
        let value = self.reader.var()?;
        usize::try_from(value)
            .map(Some)
            .map_err(|_| invalid(format!("selector index {value} is too large")))
    }

    fn selector(&mut self) -> Result<Selector, MqcError> {
        Ok(match self.reader.u8()? {
            tags::selector::BLOCKQUOTE => Selector::Blockquote,
            tags::selector::FOOTNOTE => Selector::Footnote,
            tags::selector::LIST => {
                let index = self.optional_index()?;
                let ordered = if self.reader.bool()? {
                    Some(self.reader.bool()?)
                } else {
                    None
                };
                Selector::List(index, ordered)
            }
            tags::selector::TOML => Selector::Toml,
            tags::selector::YAML => Selector::Yaml,
            tags::selector::BREAK => Selector::Break,
            tags::selector::INLINE_CODE => Selector::InlineCode,
            tags::selector::INLINE_MATH => Selector::InlineMath,
            tags::selector::DELETE => Selector::Delete,
            tags::selector::EMPHASIS => Selector::Emphasis,
            tags::selector::FOOTNOTE_REF => Selector::FootnoteRef,
            tags::selector::HTML => Selector::Html,
            tags::selector::IMAGE => Selector::Image,
            tags::selector::IMAGE_REF => Selector::ImageRef,
            tags::selector::MDX_JSX_TEXT_ELEMENT => Selector::MdxJsxTextElement,
            tags::selector::LINK => Selector::Link,
            tags::selector::LINK_REF => Selector::LinkRef,
            tags::selector::WIKI_LINK => Selector::WikiLink,
            tags::selector::CALLOUT => Selector::Callout,
            tags::selector::EMBED => Selector::Embed,
            tags::selector::STRONG => Selector::Strong,
            tags::selector::CODE => Selector::Code,
            tags::selector::MATH => Selector::Math,
            tags::selector::HEADING => Selector::Heading(if self.reader.bool()? {
                Some(self.reader.u8()?)
            } else {
                None
            }),
            tags::selector::TABLE => {
                let row = self.optional_index()?;
                let column = self.optional_index()?;
                Selector::Table(row, column)
            }
            tags::selector::TABLE_ALIGN => Selector::TableAlign,
            tags::selector::TEXT => Selector::Text,
            tags::selector::HORIZONTAL_RULE => Selector::HorizontalRule,
            tags::selector::DEFINITION => Selector::Definition,
            tags::selector::MDX_FLOW_EXPRESSION => Selector::MdxFlowExpression,
            tags::selector::MDX_TEXT_EXPRESSION => Selector::MdxTextExpression,
            tags::selector::MDX_JS_ESM => Selector::MdxJsEsm,
            tags::selector::MDX_JSX_FLOW_ELEMENT => Selector::MdxJsxFlowElement,
            tags::selector::RECURSIVE => Selector::Recursive,
            tags::selector::TASK => Selector::Task,
            tags::selector::TODO => Selector::Todo,
            tags::selector::DONE => Selector::Done,
            tags::selector::ATTR => Selector::Attr(attr_kind_from_id(self.reader.u8()?)?),
            tags::selector::PROPERTY => Selector::Property(self.ident()?),
            other => return Err(invalid(format!("unknown selector {other}"))),
        })
    }

    fn binary_op(&mut self) -> Result<BinaryOp, MqcError> {
        binary_op_from_id(self.reader.u8()?)
    }

    fn static_target(&mut self) -> Result<StaticExactCallTarget, MqcError> {
        Ok(StaticExactCallTarget {
            chunk_index: self.reader.var_u16()?,
            local_count: self.reader.var_u16()?,
        })
    }

    fn optional_u16(&mut self) -> Result<Option<u16>, MqcError> {
        Ok(if self.reader.bool()? {
            Some(self.reader.var_u16()?)
        } else {
            None
        })
    }

    fn optional_i32(&mut self) -> Result<Option<i32>, MqcError> {
        Ok(if self.reader.bool()? {
            Some(self.reader.var_i32()?)
        } else {
            None
        })
    }

    fn op(&mut self) -> Result<OpCode, MqcError> {
        let r = &mut self.reader;
        Ok(match r.u8()? {
            tags::opcode::CONST => OpCode::Const(r.var_u16()?),
            tags::opcode::PUSH_NONE => OpCode::PushNone,
            tags::opcode::GET_LOCAL => OpCode::GetLocal(r.var_u16()?),
            tags::opcode::SET_LOCAL => OpCode::SetLocal(r.var_u16()?),
            tags::opcode::SET_LOCAL_AND_COPY => OpCode::SetLocalAndCopy {
                source: r.var_u16()?,
                destination: r.var_u16()?,
            },
            tags::opcode::SET_LOCAL_AND_COPY_AND_JUMP => OpCode::SetLocalAndCopyAndJump {
                source: r.var_u16()?,
                destination: r.var_u16()?,
                offset: r.var_i32()?,
            },
            tags::opcode::SET_LOCAL_CONST => OpCode::SetLocalConst {
                local: r.var_u16()?,
                constant: r.var_u16()?,
            },
            tags::opcode::TEE_LOCAL => OpCode::TeeLocal(r.var_u16()?),
            tags::opcode::COPY_LOCAL => OpCode::CopyLocal {
                source: r.var_u16()?,
                destination: r.var_u16()?,
            },
            tags::opcode::GET_UPVALUE => OpCode::GetUpvalue(r.var_u16()?),
            tags::opcode::SET_UPVALUE => OpCode::SetUpvalue(r.var_u16()?),
            tags::opcode::MAKE_CLOSURE => {
                let target = r.var_u16()?;
                OpCode::MakeClosure(Box::new((target, self.upvalue_sources()?)))
            }
            tags::opcode::MAKE_STATIC_CLOSURE => OpCode::MakeStaticClosure(r.var_u16()?),
            tags::opcode::POP => OpCode::Pop,
            tags::opcode::DUP => OpCode::Dup,
            tags::opcode::JUMP => OpCode::Jump(r.var_i32()?),
            tags::opcode::JUMP_IF_FALSE => OpCode::JumpIfFalse(r.var_i32()?),
            tags::opcode::JUMP_IF_TRUE => OpCode::JumpIfTrue(r.var_i32()?),
            tags::opcode::ADD => OpCode::Add,
            tags::opcode::SUB => OpCode::Sub,
            tags::opcode::MUL => OpCode::Mul,
            tags::opcode::DIV => OpCode::Div,
            tags::opcode::MOD => OpCode::Mod,
            tags::opcode::EQ => OpCode::Eq,
            tags::opcode::NE => OpCode::Ne,
            tags::opcode::LT => OpCode::Lt,
            tags::opcode::LE => OpCode::Le,
            tags::opcode::GT => OpCode::Gt,
            tags::opcode::GE => OpCode::Ge,
            tags::opcode::BINARY_LOCAL_LOCAL => OpCode::BinaryLocalLocal {
                op: self.binary_op()?,
                left: self.reader.var_u16()?,
                right: self.reader.var_u16()?,
            },
            tags::opcode::BINARY_LOCAL_CONST => OpCode::BinaryLocalConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_u16()?,
            },
            tags::opcode::BINARY_LOCAL_NUMBER_CONST => OpCode::BinaryLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_i32()?,
            },
            tags::opcode::UPDATE_LOCAL_CONST => OpCode::UpdateLocalConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_u16()?,
            },
            tags::opcode::UPDATE_LOCAL_NUMBER_CONST => OpCode::UpdateLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_i32()?,
            },
            tags::opcode::UPDATE_LOCAL_LOCAL => OpCode::UpdateLocalLocal {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                value: self.reader.var_u16()?,
            },
            tags::opcode::JUMP_IF_FALSE_LOCAL_LOCAL => OpCode::JumpIfFalseLocalLocal {
                op: self.binary_op()?,
                left: self.reader.var_u16()?,
                right: self.reader.var_u16()?,
                offset: self.reader.var_i32()?,
            },
            tags::opcode::JUMP_IF_FALSE_LOCAL_CONST => OpCode::JumpIfFalseLocalConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_u16()?,
                offset: self.reader.var_i32()?,
            },
            tags::opcode::JUMP_IF_FALSE_LOCAL_NUMBER_CONST => OpCode::JumpIfFalseLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_i32()?,
                offset: self.reader.var_i32()?,
            },
            tags::opcode::NEG => OpCode::Neg,
            tags::opcode::NOT => OpCode::Not,
            tags::opcode::ARRAY_NEW => OpCode::ArrayNew,
            tags::opcode::ARRAY_NEW_WITH_CAPACITY_LOCAL => OpCode::ArrayNewWithCapacityLocal(r.var_u16()?),
            tags::opcode::ARRAY_PUSH => OpCode::ArrayPush,
            tags::opcode::ARRAY_SPREAD => OpCode::ArraySpread,
            tags::opcode::DICT_NEW => OpCode::DictNew,
            tags::opcode::DICT_INSERT => OpCode::DictInsert,
            tags::opcode::DICT_SPREAD => OpCode::DictSpread,
            tags::opcode::TO_FOREACH_ITERABLE => OpCode::ToForeachIterable,
            tags::opcode::ARRAY_LEN => OpCode::ArrayLen,
            tags::opcode::ARRAY_GET_AT => OpCode::ArrayGetAt,
            tags::opcode::ARRAY_LEN_LOCAL => OpCode::ArrayLenLocal(r.var_u16()?),
            tags::opcode::ARRAY_GET_LOCAL_AT => OpCode::ArrayGetLocalAt {
                array_slot: r.var_u16()?,
                index_slot: r.var_u16()?,
            },
            tags::opcode::FOREACH_NEXT => OpCode::ForeachNext {
                array_slot: r.var_u16()?,
                index_slot: r.var_u16()?,
                value_slot: r.var_u16()?,
                exit_offset: r.var_i32()?,
            },
            tags::opcode::FOREACH_COLLECT => OpCode::ForeachCollect(r.var_u16()?),
            tags::opcode::FOREACH_COLLECT_AND_JUMP => OpCode::ForeachCollectAndJump {
                slot: r.var_u16()?,
                offset: r.var_i32()?,
            },
            tags::opcode::FOREACH_BINARY_LOCAL_NUMBER_CONST_AND_JUMP => OpCode::ForeachBinaryLocalNumberConstAndJump {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_i32()?,
                accumulator_slot: self.reader.var_u16()?,
                offset: self.reader.var_i32()?,
            },
            tags::opcode::ARRAY_SLICE_FROM => OpCode::ArraySliceFrom,
            tags::opcode::DICT_GET_LOCAL_OR_FAIL => OpCode::DictGetLocalOrFail {
                subject_slot: r.var_u16()?,
                key: self.ident()?,
                value_slot: self.reader.var_u16()?,
            },
            tags::opcode::TYPE_CHECK => OpCode::TypeCheck(self.ident()?),
            tags::opcode::GET_ENV_VAR => OpCode::GetEnvVar(r.var_u16()?),
            tags::opcode::GET_EXTERNAL_GLOBAL => OpCode::GetExternalGlobal(self.ident()?),
            tags::opcode::INTERP_STRING => OpCode::InterpString(r.var_u16()?),
            tags::opcode::SELECTOR_MATCH => OpCode::SelectorMatch(Box::new(self.selector()?)),
            tags::opcode::SELECTOR_MATCH_KIND => {
                let selector = self.selector()?;
                OpCode::SelectorMatchKind(
                    NodeSelectorKind::from_selector(&selector)
                        .ok_or_else(|| invalid(format!("`{selector}` has no compact selector form")))?,
                )
            }
            tags::opcode::SELECTOR_MATCH_HEADING => OpCode::SelectorMatchHeading(r.u8()?),
            tags::opcode::SELECTOR_MATCH_WITH_ARGS => {
                let selector = self.selector()?;
                OpCode::SelectorMatchWithArgs(Box::new((selector, self.reader.var_u16()?)))
            }
            tags::opcode::CALL_BUILTIN_LOCAL => {
                let builtin = self.ident()?;
                let func = builtin::get_builtin_functions(&builtin)
                    .ok_or_else(|| invalid(format!("`{builtin}` is not a native builtin")))?;
                OpCode::CallBuiltinLocal {
                    func,
                    builtin,
                    local: self.reader.var_u16()?,
                }
            }
            tags::opcode::CALL_BUILTIN => {
                let ident = self.ident()?;
                let argc = self.reader.var_u16()?;
                match builtin::get_builtin_functions(&ident) {
                    Some(func) => OpCode::CallNative { func, ident, argc },
                    None => OpCode::CallBuiltin(ident, argc),
                }
            }
            tags::opcode::CALL_STATIC => OpCode::CallStatic(r.var_u16()?, r.var_u16()?),
            tags::opcode::CALL_STATIC_EXACT => OpCode::CallStaticExact(r.var_u16()?, r.var_u16()?),
            tags::opcode::CALL_STATIC_EXACT0 => OpCode::CallStaticExact0(self.static_target()?),
            tags::opcode::CALL_STATIC_EXACT1 => OpCode::CallStaticExact1(self.static_target()?),
            tags::opcode::CALL_STATIC_EXACT2 => OpCode::CallStaticExact2(self.static_target()?),
            tags::opcode::CALL_STATIC_IMPLICIT_SELF => OpCode::CallStaticImplicitSelf(r.var_u16()?, r.var_u16()?),
            tags::opcode::CALL_SELF => OpCode::CallSelf(r.var_u16()?),
            tags::opcode::CALL_SELF_EXACT => OpCode::CallSelfExact(r.var_u16()?),
            tags::opcode::CALL_SELF_EXACT0 => OpCode::CallSelfExact0,
            tags::opcode::CALL_SELF_EXACT1 => OpCode::CallSelfExact1,
            tags::opcode::CALL_SELF_EXACT2 => OpCode::CallSelfExact2,
            tags::opcode::CALL_SELF_IMPLICIT_SELF => OpCode::CallSelfImplicitSelf(r.var_u16()?),
            tags::opcode::CALL_LOCAL => OpCode::CallLocal(r.var_u16()?, r.var_u16()?),
            tags::opcode::CALL_UPVALUE => OpCode::CallUpvalue(r.var_u16()?, r.var_u16()?),
            tags::opcode::CALL_UPVALUE_LOCAL => OpCode::CallUpvalueLocal {
                index: r.var_u16()?,
                local: r.var_u16()?,
            },
            tags::opcode::CALL_VALUE => OpCode::CallValue(r.var_u16()?),
            tags::opcode::MAYBE_AUTO_CALL => OpCode::MaybeAutoCall,
            tags::opcode::TRY_CATCH => {
                let has_binder = r.bool()?;
                OpCode::TryCatch(Box::new(TryCatchInfo {
                    has_binder,
                    break_acc_slot: self.optional_u16()?,
                    break_completed_iteration_slot: self.optional_u16()?,
                    break_offset: self.optional_i32()?,
                    continue_offset: self.optional_i32()?,
                }))
            }
            tags::opcode::FLOW_BREAK => OpCode::FlowBreak(r.bool()?),
            tags::opcode::FLOW_CONTINUE => OpCode::FlowContinue,
            tags::opcode::RAISE_DESTRUCTURING_FAILED => OpCode::RaiseDestructuringFailed,
            tags::opcode::RETURN_LOCAL => OpCode::ReturnLocal(r.var_u16()?),
            tags::opcode::RETURN_BINARY_LOCAL_LOCAL => OpCode::ReturnBinaryLocalLocal {
                op: self.binary_op()?,
                left: self.reader.var_u16()?,
                right: self.reader.var_u16()?,
            },
            tags::opcode::RETURN_BINARY_LOCAL_CONST => OpCode::ReturnBinaryLocalConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_u16()?,
            },
            tags::opcode::RETURN_BINARY_LOCAL_NUMBER_CONST => OpCode::ReturnBinaryLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.var_u16()?,
                constant: self.reader.var_i32()?,
            },
            tags::opcode::RETURN => OpCode::Return,
            tags::opcode::YIELD => OpCode::Yield,
            tags::opcode::RESUME => OpCode::Resume(r.u8()?),
            other => return Err(invalid(format!("unknown instruction {other}"))),
        })
    }
}

/// The wire id `op` is encoded with.
#[cfg(test)]
pub(super) fn instruction_id(op: &OpCode) -> u8 {
    let mut writer = Writer::default();
    encode_op(&mut writer, &mut Tables::default(), op).expect("encodable instruction");
    writer.into_bytes()[0]
}

/// Every wire id the decoder accepts as an instruction.
#[cfg(test)]
pub(super) fn known_instruction_ids() -> Vec<u8> {
    (0..=u8::MAX)
        .filter(|id| {
            let mut decoder = Decoder {
                reader: Reader::new(std::slice::from_ref(id)),
                idents: Vec::new(),
                tokens: &[],
            };
            // Operands are missing, so only an unknown id fails with this message.
            !matches!(decoder.op(), Err(MqcError::InvalidBytecode(message)) if message.starts_with("unknown instruction"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::markdown(RuntimeValue::new_markdown(mq_markdown::Node::Empty))]
    #[case::nested(RuntimeValue::Array(Shared::new(vec![RuntimeValue::new_markdown(mq_markdown::Node::Empty)])))]
    fn test_encode_constant_rejects_runtime_only_values(#[case] value: RuntimeValue) {
        let result = encode_constant(&mut Writer::default(), &mut Tables::default(), &value, 0);
        assert!(matches!(result, Err(MqcError::UnsupportedValue(_))));
    }

    #[test]
    fn test_selectors_round_trip() {
        let selectors = [
            Selector::List(Some(3), Some(true)),
            Selector::List(None, Some(false)),
            Selector::Heading(Some(2)),
            Selector::Heading(None),
            Selector::Table(Some(1), None),
            Selector::Table(None, Some(4)),
            Selector::Attr(AttrKind::EndLine),
            Selector::Property(Ident::new("key")),
            Selector::Recursive,
        ];
        let mut tables = Tables::default();
        let mut writer = Writer::default();
        for selector in &selectors {
            encode_selector(&mut writer, &mut tables, selector).unwrap();
        }
        let bytes = writer.into_bytes();
        let mut decoder = Decoder {
            reader: Reader::new(&bytes),
            idents: tables.idents,
            tokens: &[],
        };
        for selector in &selectors {
            assert_eq!(&decoder.selector().unwrap(), selector);
        }
    }

    #[test]
    fn test_attr_kind_ids_round_trip() {
        for id in 0..=22 {
            assert_eq!(attr_kind_id(&attr_kind_from_id(id).unwrap()), id);
        }
        assert!(attr_kind_from_id(23).is_err());
    }
}
