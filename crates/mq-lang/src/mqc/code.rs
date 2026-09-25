//! `.mqc` CODE section codec. Instructions use fixed wire IDs; decoded code is verified.
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
        writer.u32(index);
    }

    fn token(&mut self, writer: &mut Writer, token_id: TokenId) {
        let next = self.tokens.len() as u32;
        let index = *self.token_indexes.entry(token_id).or_insert_with(|| {
            self.tokens.push(token_id);
            next
        });
        writer.u32(index);
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

    let mut payload = Writer::default();
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
    let ident_count = reader.len(4)?;
    let idents = (0..ident_count)
        .map(|_| reader.string().map(|name| Ident::new(&name)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut decoder = Decoder { reader, idents, tokens };

    let let_name_count = decoder.reader.len(4)?;
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
        writer.u16(closure.chunk_index);
    }

    writer.len(chunk.code.len())?;
    for op in &chunk.code {
        encode_op(writer, tables, op)?;
    }
    writer.len(chunk.lines.len())?;
    for line in &chunk.lines {
        writer.len(line.pc_start)?;
        tables.token(writer, line.token_id);
    }
    Ok(())
}

fn encode_param_shape(writer: &mut Writer, shape: &ParamShape) -> Result<(), MqcError> {
    writer.len(shape.bindings.len())?;
    for binding in &shape.bindings {
        match binding {
            ParamBinding::Required(slot) => {
                writer.u8(0);
                writer.u16(*slot);
            }
            ParamBinding::Optional(slot, default_chunk, sources) => {
                writer.u8(1);
                writer.u16(*slot);
                writer.u16(*default_chunk);
                encode_upvalue_sources(writer, sources)?;
            }
            ParamBinding::Variadic(slot) => {
                writer.u8(2);
                writer.u16(*slot);
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
                writer.u8(0);
                writer.u16(*slot);
            }
            UpvalueSource::Upvalue(index) => {
                writer.u8(1);
                writer.u16(*index);
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
        RuntimeValue::None => writer.u8(0),
        RuntimeValue::Number(number) => {
            writer.u8(1);
            writer.f64(number.value());
        }
        RuntimeValue::Boolean(value) => {
            writer.u8(2);
            writer.bool(*value);
        }
        RuntimeValue::String(value) => {
            writer.u8(3);
            writer.str(value)?;
        }
        RuntimeValue::Symbol(ident) => {
            writer.u8(4);
            tables.ident(writer, *ident);
        }
        RuntimeValue::Bytes(bytes) => {
            writer.u8(5);
            writer.bytes(bytes)?;
        }
        RuntimeValue::Array(values) => {
            writer.u8(6);
            writer.len(values.len())?;
            for value in values.iter() {
                encode_constant(writer, tables, value, depth + 1)?;
            }
        }
        RuntimeValue::Dict(map) => {
            writer.u8(7);
            writer.len(map.len())?;
            for (key, value) in map.iter() {
                tables.ident(writer, *key);
                encode_constant(writer, tables, value, depth + 1)?;
            }
        }
        RuntimeValue::NativeFunction(ident) => {
            writer.u8(8);
            tables.note_builtin(*ident);
            tables.ident(writer, *ident);
        }
        RuntimeValue::CoroutineBuiltin(builtin) => {
            writer.u8(9);
            writer.u8(match builtin {
                ResumeBuiltin::Next => 0,
                ResumeBuiltin::Send => 1,
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
        BinaryOp::Add => 0,
        BinaryOp::Sub => 1,
        BinaryOp::Mul => 2,
        BinaryOp::Div => 3,
        BinaryOp::Mod => 4,
        BinaryOp::Eq => 5,
        BinaryOp::Ne => 6,
        BinaryOp::Lt => 7,
        BinaryOp::Le => 8,
        BinaryOp::Gt => 9,
        BinaryOp::Ge => 10,
    }
}

fn binary_op_from_id(id: u8) -> Result<BinaryOp, MqcError> {
    Ok(match id {
        0 => BinaryOp::Add,
        1 => BinaryOp::Sub,
        2 => BinaryOp::Mul,
        3 => BinaryOp::Div,
        4 => BinaryOp::Mod,
        5 => BinaryOp::Eq,
        6 => BinaryOp::Ne,
        7 => BinaryOp::Lt,
        8 => BinaryOp::Le,
        9 => BinaryOp::Gt,
        10 => BinaryOp::Ge,
        other => return Err(invalid(format!("unknown binary operator {other}"))),
    })
}

fn attr_kind_id(kind: &AttrKind) -> u8 {
    match kind {
        AttrKind::Value => 0,
        AttrKind::Values => 1,
        AttrKind::Children => 2,
        AttrKind::Lang => 3,
        AttrKind::Meta => 4,
        AttrKind::Fence => 5,
        AttrKind::Url => 6,
        AttrKind::Alt => 7,
        AttrKind::Title => 8,
        AttrKind::Ident => 9,
        AttrKind::Label => 10,
        AttrKind::Depth => 11,
        AttrKind::Level => 12,
        AttrKind::Index => 13,
        AttrKind::Ordered => 14,
        AttrKind::Checked => 15,
        AttrKind::Column => 16,
        AttrKind::Row => 17,
        AttrKind::Align => 18,
        AttrKind::Name => 19,
        AttrKind::Kind => 20,
        AttrKind::Line => 21,
        AttrKind::EndLine => 22,
    }
}

fn attr_kind_from_id(id: u8) -> Result<AttrKind, MqcError> {
    Ok(match id {
        0 => AttrKind::Value,
        1 => AttrKind::Values,
        2 => AttrKind::Children,
        3 => AttrKind::Lang,
        4 => AttrKind::Meta,
        5 => AttrKind::Fence,
        6 => AttrKind::Url,
        7 => AttrKind::Alt,
        8 => AttrKind::Title,
        9 => AttrKind::Ident,
        10 => AttrKind::Label,
        11 => AttrKind::Depth,
        12 => AttrKind::Level,
        13 => AttrKind::Index,
        14 => AttrKind::Ordered,
        15 => AttrKind::Checked,
        16 => AttrKind::Column,
        17 => AttrKind::Row,
        18 => AttrKind::Align,
        19 => AttrKind::Name,
        20 => AttrKind::Kind,
        21 => AttrKind::Line,
        22 => AttrKind::EndLine,
        other => return Err(invalid(format!("unknown attribute selector {other}"))),
    })
}

fn encode_optional_index(writer: &mut Writer, value: Option<usize>) -> Result<(), MqcError> {
    match value {
        Some(value) => {
            writer.bool(true);
            let value = u64::try_from(value).map_err(|_| MqcError::UnsupportedValue("a selector index".into()))?;
            writer.u64(value);
        }
        None => writer.bool(false),
    }
    Ok(())
}

fn encode_selector(writer: &mut Writer, tables: &mut Tables, selector: &Selector) -> Result<(), MqcError> {
    match selector {
        Selector::Blockquote => writer.u8(0),
        Selector::Footnote => writer.u8(1),
        Selector::List(index, ordered) => {
            writer.u8(2);
            encode_optional_index(writer, *index)?;
            match ordered {
                Some(ordered) => {
                    writer.bool(true);
                    writer.bool(*ordered);
                }
                None => writer.bool(false),
            }
        }
        Selector::Toml => writer.u8(3),
        Selector::Yaml => writer.u8(4),
        Selector::Break => writer.u8(5),
        Selector::InlineCode => writer.u8(6),
        Selector::InlineMath => writer.u8(7),
        Selector::Delete => writer.u8(8),
        Selector::Emphasis => writer.u8(9),
        Selector::FootnoteRef => writer.u8(10),
        Selector::Html => writer.u8(11),
        Selector::Image => writer.u8(12),
        Selector::ImageRef => writer.u8(13),
        Selector::MdxJsxTextElement => writer.u8(14),
        Selector::Link => writer.u8(15),
        Selector::LinkRef => writer.u8(16),
        Selector::WikiLink => writer.u8(17),
        Selector::Callout => writer.u8(18),
        Selector::Embed => writer.u8(19),
        Selector::Strong => writer.u8(20),
        Selector::Code => writer.u8(21),
        Selector::Math => writer.u8(22),
        Selector::Heading(level) => {
            writer.u8(23);
            match level {
                Some(level) => {
                    writer.bool(true);
                    writer.u8(*level);
                }
                None => writer.bool(false),
            }
        }
        Selector::Table(row, column) => {
            writer.u8(24);
            encode_optional_index(writer, *row)?;
            encode_optional_index(writer, *column)?;
        }
        Selector::TableAlign => writer.u8(25),
        Selector::Text => writer.u8(26),
        Selector::HorizontalRule => writer.u8(27),
        Selector::Definition => writer.u8(28),
        Selector::MdxFlowExpression => writer.u8(29),
        Selector::MdxTextExpression => writer.u8(30),
        Selector::MdxJsEsm => writer.u8(31),
        Selector::MdxJsxFlowElement => writer.u8(32),
        Selector::Recursive => writer.u8(33),
        Selector::Task => writer.u8(34),
        Selector::Todo => writer.u8(35),
        Selector::Done => writer.u8(36),
        Selector::Attr(kind) => {
            writer.u8(37);
            writer.u8(attr_kind_id(kind));
        }
        Selector::Property(ident) => {
            writer.u8(38);
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
            writer.u8(1);
            writer.u16(*index);
        }
        OpCode::PushNone => writer.u8(2),
        OpCode::GetLocal(slot) => {
            writer.u8(3);
            writer.u16(*slot);
        }
        OpCode::SetLocal(slot) => {
            writer.u8(4);
            writer.u16(*slot);
        }
        OpCode::SetLocalAndCopy { source, destination } => {
            writer.u8(5);
            writer.u16(*source);
            writer.u16(*destination);
        }
        OpCode::SetLocalAndCopyAndJump {
            source,
            destination,
            offset,
        } => {
            writer.u8(6);
            writer.u16(*source);
            writer.u16(*destination);
            writer.i32(*offset);
        }
        OpCode::SetLocalConst { local, constant } => {
            writer.u8(7);
            writer.u16(*local);
            writer.u16(*constant);
        }
        OpCode::TeeLocal(slot) => {
            writer.u8(8);
            writer.u16(*slot);
        }
        OpCode::CopyLocal { source, destination } => {
            writer.u8(9);
            writer.u16(*source);
            writer.u16(*destination);
        }
        OpCode::GetUpvalue(index) => {
            writer.u8(10);
            writer.u16(*index);
        }
        OpCode::SetUpvalue(index) => {
            writer.u8(11);
            writer.u16(*index);
        }
        OpCode::MakeClosure(payload) => {
            writer.u8(12);
            writer.u16(payload.0);
            encode_upvalue_sources(writer, &payload.1)?;
        }
        OpCode::MakeStaticClosure(index) => {
            writer.u8(13);
            writer.u16(*index);
        }
        OpCode::Pop => writer.u8(14),
        OpCode::Dup => writer.u8(15),
        OpCode::Jump(offset) => {
            writer.u8(16);
            writer.i32(*offset);
        }
        OpCode::JumpIfFalse(offset) => {
            writer.u8(17);
            writer.i32(*offset);
        }
        OpCode::Add => writer.u8(18),
        OpCode::Sub => writer.u8(19),
        OpCode::Mul => writer.u8(20),
        OpCode::Div => writer.u8(21),
        OpCode::Mod => writer.u8(22),
        OpCode::Eq => writer.u8(23),
        OpCode::Ne => writer.u8(24),
        OpCode::Lt => writer.u8(25),
        OpCode::Le => writer.u8(26),
        OpCode::Gt => writer.u8(27),
        OpCode::Ge => writer.u8(28),
        OpCode::BinaryLocalLocal { op, left, right } => {
            writer.u8(29);
            writer.u8(binary_op_id(*op));
            writer.u16(*left);
            writer.u16(*right);
        }
        OpCode::BinaryLocalConst { op, local, constant } => {
            writer.u8(30);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.u16(*constant);
        }
        OpCode::BinaryLocalNumberConst { op, local, constant } => {
            writer.u8(31);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.i32(*constant);
        }
        OpCode::UpdateLocalConst { op, local, constant } => {
            writer.u8(32);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.u16(*constant);
        }
        OpCode::UpdateLocalNumberConst { op, local, constant } => {
            writer.u8(33);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.i32(*constant);
        }
        OpCode::UpdateLocalLocal { op, local, value } => {
            writer.u8(34);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.u16(*value);
        }
        OpCode::JumpIfFalseLocalLocal {
            op,
            left,
            right,
            offset,
        } => {
            writer.u8(35);
            writer.u8(binary_op_id(*op));
            writer.u16(*left);
            writer.u16(*right);
            writer.i32(*offset);
        }
        OpCode::JumpIfFalseLocalConst {
            op,
            local,
            constant,
            offset,
        } => {
            writer.u8(36);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.u16(*constant);
            writer.i32(*offset);
        }
        OpCode::JumpIfFalseLocalNumberConst {
            op,
            local,
            constant,
            offset,
        } => {
            writer.u8(37);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.i32(*constant);
            writer.i32(*offset);
        }
        OpCode::Neg => writer.u8(38),
        OpCode::Not => writer.u8(39),
        OpCode::ArrayNew => writer.u8(40),
        OpCode::ArrayNewWithCapacityLocal(slot) => {
            writer.u8(41);
            writer.u16(*slot);
        }
        OpCode::ArrayPush => writer.u8(42),
        OpCode::ArraySpread => writer.u8(43),
        OpCode::DictNew => writer.u8(44),
        OpCode::DictInsert => writer.u8(45),
        OpCode::DictSpread => writer.u8(46),
        OpCode::ToForeachIterable => writer.u8(47),
        OpCode::ArrayLen => writer.u8(48),
        OpCode::ArrayGetAt => writer.u8(49),
        OpCode::ArrayLenLocal(slot) => {
            writer.u8(50);
            writer.u16(*slot);
        }
        OpCode::ArrayGetLocalAt { array_slot, index_slot } => {
            writer.u8(51);
            writer.u16(*array_slot);
            writer.u16(*index_slot);
        }
        OpCode::ForeachNext {
            array_slot,
            index_slot,
            value_slot,
            exit_offset,
        } => {
            writer.u8(52);
            writer.u16(*array_slot);
            writer.u16(*index_slot);
            writer.u16(*value_slot);
            writer.i32(*exit_offset);
        }
        OpCode::ForeachCollect(slot) => {
            writer.u8(53);
            writer.u16(*slot);
        }
        OpCode::ForeachCollectAndJump { slot, offset } => {
            writer.u8(54);
            writer.u16(*slot);
            writer.i32(*offset);
        }
        OpCode::ForeachBinaryLocalNumberConstAndJump {
            op,
            local,
            constant,
            accumulator_slot,
            offset,
        } => {
            writer.u8(55);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.i32(*constant);
            writer.u16(*accumulator_slot);
            writer.i32(*offset);
        }
        OpCode::ArraySliceFrom => writer.u8(56),
        OpCode::DictGetLocalOrFail {
            subject_slot,
            key,
            value_slot,
        } => {
            writer.u8(57);
            writer.u16(*subject_slot);
            tables.ident(writer, *key);
            writer.u16(*value_slot);
        }
        OpCode::TypeCheck(ident) => {
            writer.u8(58);
            tables.ident(writer, *ident);
        }
        OpCode::GetEnvVar(index) => {
            writer.u8(59);
            writer.u16(*index);
        }
        OpCode::GetExternalGlobal(ident) => {
            writer.u8(60);
            tables.note_external_global(*ident);
            tables.ident(writer, *ident);
        }
        OpCode::InterpString(count) => {
            writer.u8(61);
            writer.u16(*count);
        }
        OpCode::SelectorMatch(selector) => {
            writer.u8(62);
            encode_selector(writer, tables, selector)?;
        }
        OpCode::SelectorMatchKind(kind) => {
            writer.u8(63);
            encode_selector(writer, tables, &kind.as_selector())?;
        }
        OpCode::SelectorMatchHeading(level) => {
            writer.u8(64);
            writer.u8(*level);
        }
        OpCode::SelectorMatchWithArgs(payload) => {
            writer.u8(65);
            encode_selector(writer, tables, &payload.0)?;
            writer.u16(payload.1);
        }
        OpCode::CallBuiltinLocal { builtin, local } => {
            writer.u8(66);
            tables.note_builtin(*builtin);
            tables.ident(writer, *builtin);
            writer.u16(*local);
        }
        OpCode::CallBuiltin(ident, argc) => {
            writer.u8(67);
            tables.note_builtin(*ident);
            tables.ident(writer, *ident);
            writer.u16(*argc);
        }
        OpCode::CallStatic(target, argc) => {
            writer.u8(68);
            writer.u16(*target);
            writer.u16(*argc);
        }
        OpCode::CallStaticExact(target, argc) => {
            writer.u8(69);
            writer.u16(*target);
            writer.u16(*argc);
        }
        OpCode::CallStaticExact0(target) => {
            writer.u8(70);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticExact1(target) => {
            writer.u8(71);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticExact2(target) => {
            writer.u8(72);
            encode_static_target(writer, target);
        }
        OpCode::CallStaticImplicitSelf(target, argc) => {
            writer.u8(73);
            writer.u16(*target);
            writer.u16(*argc);
        }
        OpCode::CallSelf(argc) => {
            writer.u8(74);
            writer.u16(*argc);
        }
        OpCode::CallSelfExact(argc) => {
            writer.u8(75);
            writer.u16(*argc);
        }
        OpCode::CallSelfExact0 => writer.u8(76),
        OpCode::CallSelfExact1 => writer.u8(77),
        OpCode::CallSelfExact2 => writer.u8(78),
        OpCode::CallSelfImplicitSelf(argc) => {
            writer.u8(79);
            writer.u16(*argc);
        }
        OpCode::CallLocal(slot, argc) => {
            writer.u8(80);
            writer.u16(*slot);
            writer.u16(*argc);
        }
        OpCode::CallUpvalue(index, argc) => {
            writer.u8(81);
            writer.u16(*index);
            writer.u16(*argc);
        }
        OpCode::CallUpvalueLocal { index, local } => {
            writer.u8(82);
            writer.u16(*index);
            writer.u16(*local);
        }
        OpCode::CallValue(argc) => {
            writer.u8(83);
            writer.u16(*argc);
        }
        OpCode::MaybeAutoCall => writer.u8(84),
        OpCode::TryCatch(info) => {
            writer.u8(85);
            writer.bool(info.has_binder);
            encode_optional_u16(writer, info.break_acc_slot);
            encode_optional_u16(writer, info.break_completed_iteration_slot);
            encode_optional_i32(writer, info.break_offset);
            encode_optional_i32(writer, info.continue_offset);
        }
        OpCode::FlowBreak(has_value) => {
            writer.u8(86);
            writer.bool(*has_value);
        }
        OpCode::FlowContinue => writer.u8(87),
        OpCode::RaiseDestructuringFailed => writer.u8(88),
        OpCode::ReturnLocal(slot) => {
            writer.u8(89);
            writer.u16(*slot);
        }
        OpCode::ReturnBinaryLocalLocal { op, left, right } => {
            writer.u8(90);
            writer.u8(binary_op_id(*op));
            writer.u16(*left);
            writer.u16(*right);
        }
        OpCode::ReturnBinaryLocalConst { op, local, constant } => {
            writer.u8(91);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.u16(*constant);
        }
        OpCode::ReturnBinaryLocalNumberConst { op, local, constant } => {
            writer.u8(92);
            writer.u8(binary_op_id(*op));
            writer.u16(*local);
            writer.i32(*constant);
        }
        OpCode::Return => writer.u8(93),
        OpCode::Yield => writer.u8(94),
        OpCode::Resume(argc) => {
            writer.u8(95);
            writer.u8(*argc);
        }
    }
    Ok(())
}

fn encode_static_target(writer: &mut Writer, target: &StaticExactCallTarget) {
    writer.u16(target.chunk_index);
    writer.u16(target.local_count);
}

fn encode_optional_u16(writer: &mut Writer, value: Option<u16>) {
    match value {
        Some(value) => {
            writer.bool(true);
            writer.u16(value);
        }
        None => writer.bool(false),
    }
}

fn encode_optional_i32(writer: &mut Writer, value: Option<i32>) {
    match value {
        Some(value) => {
            writer.bool(true);
            writer.i32(value);
        }
        None => writer.bool(false),
    }
}

fn invalid(message: String) -> MqcError {
    MqcError::InvalidBytecode(message)
}

struct Decoder<'a> {
    reader: Reader<'a>,
    idents: Vec<Ident>,
    tokens: &'a [TokenId],
}

impl Decoder<'_> {
    fn ident(&mut self) -> Result<Ident, MqcError> {
        let index = self.reader.u32()? as usize;
        self.idents
            .get(index)
            .copied()
            .ok_or_else(|| invalid(format!("identifier {index} is out of bounds")))
    }

    fn token(&mut self) -> Result<TokenId, MqcError> {
        let index = self.reader.u32()? as usize;
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

        let local_count = self.reader.len(5)?;
        let local_count_u16 =
            u16::try_from(local_count).map_err(|_| invalid(format!("{local_count} locals exceed the VM limit")))?;
        let mut local_names = Vec::with_capacity(local_count);
        let mut local_mutable = Vec::with_capacity(local_count);
        for _ in 0..local_count {
            local_names.push(self.ident()?);
            local_mutable.push(self.reader.bool()?);
        }
        let upvalue_count = self.reader.len(4)?;
        let upvalue_names = (0..upvalue_count)
            .map(|_| self.ident())
            .collect::<Result<Vec<_>, _>>()?;

        let param_shape = self.param_shape()?;

        let constant_count = self.reader.len(1)?;
        let constants = (0..constant_count)
            .map(|_| self.constant(0))
            .collect::<Result<Vec<_>, _>>()?;
        let static_closure_count = self.reader.len(2)?;
        let mut chunk = Chunk::default();
        for _ in 0..static_closure_count {
            let target = self.reader.u16()?;
            chunk.push_static_closure(target);
        }

        let op_count = self.reader.len(1)?;
        let code = (0..op_count).map(|_| self.op()).collect::<Result<Vec<_>, _>>()?;
        let line_count = self.reader.len(8)?;
        let mut lines = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let pc_start = self.reader.u32()? as usize;
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
        let count = self.reader.len(3)?;
        let mut bindings = Vec::with_capacity(count);
        let mut required = 0;
        let mut has_variadic = false;
        for _ in 0..count {
            if has_variadic {
                return Err(invalid("a parameter follows a variadic parameter".to_string()));
            }
            let binding = match self.reader.u8()? {
                0 => {
                    if bindings.len() != required {
                        return Err(invalid("a required parameter follows an optional one".to_string()));
                    }
                    required += 1;
                    ParamBinding::Required(self.reader.u16()?)
                }
                1 => {
                    let slot = self.reader.u16()?;
                    let default_chunk = self.reader.u16()?;
                    ParamBinding::Optional(slot, default_chunk, self.upvalue_sources()?)
                }
                2 => {
                    has_variadic = true;
                    ParamBinding::Variadic(self.reader.u16()?)
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
        let count = self.reader.len(3)?;
        (0..count)
            .map(|_| match self.reader.u8()? {
                0 => Ok(UpvalueSource::Local(self.reader.u16()?)),
                1 => Ok(UpvalueSource::Upvalue(self.reader.u16()?)),
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
            0 => RuntimeValue::None,
            1 => RuntimeValue::Number(self.reader.f64()?.into()),
            2 => RuntimeValue::Boolean(self.reader.bool()?),
            3 => RuntimeValue::String(Shared::new(self.reader.string()?)),
            4 => RuntimeValue::Symbol(self.ident()?),
            5 => RuntimeValue::Bytes(Shared::new(self.reader.bytes()?.to_vec())),
            6 => {
                let count = self.reader.len(1)?;
                let values = (0..count)
                    .map(|_| self.constant(depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                RuntimeValue::Array(Shared::new(values))
            }
            7 => {
                let count = self.reader.len(5)?;
                let mut map = DictMap::default();
                for _ in 0..count {
                    let key = self.ident()?;
                    let value = self.constant(depth + 1)?;
                    map.insert(key, value);
                }
                RuntimeValue::Dict(Shared::new(map))
            }
            8 => RuntimeValue::NativeFunction(self.ident()?),
            9 => RuntimeValue::CoroutineBuiltin(match self.reader.u8()? {
                0 => ResumeBuiltin::Next,
                1 => ResumeBuiltin::Send,
                other => return Err(invalid(format!("unknown coroutine builtin {other}"))),
            }),
            other => return Err(invalid(format!("unknown constant tag {other}"))),
        })
    }

    fn optional_index(&mut self) -> Result<Option<usize>, MqcError> {
        if !self.reader.bool()? {
            return Ok(None);
        }
        let value = self.reader.u64()?;
        usize::try_from(value)
            .map(Some)
            .map_err(|_| invalid(format!("selector index {value} is too large")))
    }

    fn selector(&mut self) -> Result<Selector, MqcError> {
        Ok(match self.reader.u8()? {
            0 => Selector::Blockquote,
            1 => Selector::Footnote,
            2 => {
                let index = self.optional_index()?;
                let ordered = if self.reader.bool()? {
                    Some(self.reader.bool()?)
                } else {
                    None
                };
                Selector::List(index, ordered)
            }
            3 => Selector::Toml,
            4 => Selector::Yaml,
            5 => Selector::Break,
            6 => Selector::InlineCode,
            7 => Selector::InlineMath,
            8 => Selector::Delete,
            9 => Selector::Emphasis,
            10 => Selector::FootnoteRef,
            11 => Selector::Html,
            12 => Selector::Image,
            13 => Selector::ImageRef,
            14 => Selector::MdxJsxTextElement,
            15 => Selector::Link,
            16 => Selector::LinkRef,
            17 => Selector::WikiLink,
            18 => Selector::Callout,
            19 => Selector::Embed,
            20 => Selector::Strong,
            21 => Selector::Code,
            22 => Selector::Math,
            23 => Selector::Heading(if self.reader.bool()? {
                Some(self.reader.u8()?)
            } else {
                None
            }),
            24 => {
                let row = self.optional_index()?;
                let column = self.optional_index()?;
                Selector::Table(row, column)
            }
            25 => Selector::TableAlign,
            26 => Selector::Text,
            27 => Selector::HorizontalRule,
            28 => Selector::Definition,
            29 => Selector::MdxFlowExpression,
            30 => Selector::MdxTextExpression,
            31 => Selector::MdxJsEsm,
            32 => Selector::MdxJsxFlowElement,
            33 => Selector::Recursive,
            34 => Selector::Task,
            35 => Selector::Todo,
            36 => Selector::Done,
            37 => Selector::Attr(attr_kind_from_id(self.reader.u8()?)?),
            38 => Selector::Property(self.ident()?),
            other => return Err(invalid(format!("unknown selector {other}"))),
        })
    }

    fn binary_op(&mut self) -> Result<BinaryOp, MqcError> {
        binary_op_from_id(self.reader.u8()?)
    }

    fn static_target(&mut self) -> Result<StaticExactCallTarget, MqcError> {
        Ok(StaticExactCallTarget {
            chunk_index: self.reader.u16()?,
            local_count: self.reader.u16()?,
        })
    }

    fn optional_u16(&mut self) -> Result<Option<u16>, MqcError> {
        Ok(if self.reader.bool()? {
            Some(self.reader.u16()?)
        } else {
            None
        })
    }

    fn optional_i32(&mut self) -> Result<Option<i32>, MqcError> {
        Ok(if self.reader.bool()? {
            Some(self.reader.i32()?)
        } else {
            None
        })
    }

    fn op(&mut self) -> Result<OpCode, MqcError> {
        let r = &mut self.reader;
        Ok(match r.u8()? {
            1 => OpCode::Const(r.u16()?),
            2 => OpCode::PushNone,
            3 => OpCode::GetLocal(r.u16()?),
            4 => OpCode::SetLocal(r.u16()?),
            5 => OpCode::SetLocalAndCopy {
                source: r.u16()?,
                destination: r.u16()?,
            },
            6 => OpCode::SetLocalAndCopyAndJump {
                source: r.u16()?,
                destination: r.u16()?,
                offset: r.i32()?,
            },
            7 => OpCode::SetLocalConst {
                local: r.u16()?,
                constant: r.u16()?,
            },
            8 => OpCode::TeeLocal(r.u16()?),
            9 => OpCode::CopyLocal {
                source: r.u16()?,
                destination: r.u16()?,
            },
            10 => OpCode::GetUpvalue(r.u16()?),
            11 => OpCode::SetUpvalue(r.u16()?),
            12 => {
                let target = r.u16()?;
                OpCode::MakeClosure(Box::new((target, self.upvalue_sources()?)))
            }
            13 => OpCode::MakeStaticClosure(r.u16()?),
            14 => OpCode::Pop,
            15 => OpCode::Dup,
            16 => OpCode::Jump(r.i32()?),
            17 => OpCode::JumpIfFalse(r.i32()?),
            18 => OpCode::Add,
            19 => OpCode::Sub,
            20 => OpCode::Mul,
            21 => OpCode::Div,
            22 => OpCode::Mod,
            23 => OpCode::Eq,
            24 => OpCode::Ne,
            25 => OpCode::Lt,
            26 => OpCode::Le,
            27 => OpCode::Gt,
            28 => OpCode::Ge,
            29 => OpCode::BinaryLocalLocal {
                op: self.binary_op()?,
                left: self.reader.u16()?,
                right: self.reader.u16()?,
            },
            30 => OpCode::BinaryLocalConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.u16()?,
            },
            31 => OpCode::BinaryLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.i32()?,
            },
            32 => OpCode::UpdateLocalConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.u16()?,
            },
            33 => OpCode::UpdateLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.i32()?,
            },
            34 => OpCode::UpdateLocalLocal {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                value: self.reader.u16()?,
            },
            35 => OpCode::JumpIfFalseLocalLocal {
                op: self.binary_op()?,
                left: self.reader.u16()?,
                right: self.reader.u16()?,
                offset: self.reader.i32()?,
            },
            36 => OpCode::JumpIfFalseLocalConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.u16()?,
                offset: self.reader.i32()?,
            },
            37 => OpCode::JumpIfFalseLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.i32()?,
                offset: self.reader.i32()?,
            },
            38 => OpCode::Neg,
            39 => OpCode::Not,
            40 => OpCode::ArrayNew,
            41 => OpCode::ArrayNewWithCapacityLocal(r.u16()?),
            42 => OpCode::ArrayPush,
            43 => OpCode::ArraySpread,
            44 => OpCode::DictNew,
            45 => OpCode::DictInsert,
            46 => OpCode::DictSpread,
            47 => OpCode::ToForeachIterable,
            48 => OpCode::ArrayLen,
            49 => OpCode::ArrayGetAt,
            50 => OpCode::ArrayLenLocal(r.u16()?),
            51 => OpCode::ArrayGetLocalAt {
                array_slot: r.u16()?,
                index_slot: r.u16()?,
            },
            52 => OpCode::ForeachNext {
                array_slot: r.u16()?,
                index_slot: r.u16()?,
                value_slot: r.u16()?,
                exit_offset: r.i32()?,
            },
            53 => OpCode::ForeachCollect(r.u16()?),
            54 => OpCode::ForeachCollectAndJump {
                slot: r.u16()?,
                offset: r.i32()?,
            },
            55 => OpCode::ForeachBinaryLocalNumberConstAndJump {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.i32()?,
                accumulator_slot: self.reader.u16()?,
                offset: self.reader.i32()?,
            },
            56 => OpCode::ArraySliceFrom,
            57 => OpCode::DictGetLocalOrFail {
                subject_slot: r.u16()?,
                key: self.ident()?,
                value_slot: self.reader.u16()?,
            },
            58 => OpCode::TypeCheck(self.ident()?),
            59 => OpCode::GetEnvVar(r.u16()?),
            60 => OpCode::GetExternalGlobal(self.ident()?),
            61 => OpCode::InterpString(r.u16()?),
            62 => OpCode::SelectorMatch(Box::new(self.selector()?)),
            63 => {
                let selector = self.selector()?;
                OpCode::SelectorMatchKind(
                    NodeSelectorKind::from_selector(&selector)
                        .ok_or_else(|| invalid(format!("`{selector}` has no compact selector form")))?,
                )
            }
            64 => OpCode::SelectorMatchHeading(r.u8()?),
            65 => {
                let selector = self.selector()?;
                OpCode::SelectorMatchWithArgs(Box::new((selector, self.reader.u16()?)))
            }
            66 => OpCode::CallBuiltinLocal {
                builtin: self.ident()?,
                local: self.reader.u16()?,
            },
            67 => OpCode::CallBuiltin(self.ident()?, self.reader.u16()?),
            68 => OpCode::CallStatic(r.u16()?, r.u16()?),
            69 => OpCode::CallStaticExact(r.u16()?, r.u16()?),
            70 => OpCode::CallStaticExact0(self.static_target()?),
            71 => OpCode::CallStaticExact1(self.static_target()?),
            72 => OpCode::CallStaticExact2(self.static_target()?),
            73 => OpCode::CallStaticImplicitSelf(r.u16()?, r.u16()?),
            74 => OpCode::CallSelf(r.u16()?),
            75 => OpCode::CallSelfExact(r.u16()?),
            76 => OpCode::CallSelfExact0,
            77 => OpCode::CallSelfExact1,
            78 => OpCode::CallSelfExact2,
            79 => OpCode::CallSelfImplicitSelf(r.u16()?),
            80 => OpCode::CallLocal(r.u16()?, r.u16()?),
            81 => OpCode::CallUpvalue(r.u16()?, r.u16()?),
            82 => OpCode::CallUpvalueLocal {
                index: r.u16()?,
                local: r.u16()?,
            },
            83 => OpCode::CallValue(r.u16()?),
            84 => OpCode::MaybeAutoCall,
            85 => {
                let has_binder = r.bool()?;
                OpCode::TryCatch(Box::new(TryCatchInfo {
                    has_binder,
                    break_acc_slot: self.optional_u16()?,
                    break_completed_iteration_slot: self.optional_u16()?,
                    break_offset: self.optional_i32()?,
                    continue_offset: self.optional_i32()?,
                }))
            }
            86 => OpCode::FlowBreak(r.bool()?),
            87 => OpCode::FlowContinue,
            88 => OpCode::RaiseDestructuringFailed,
            89 => OpCode::ReturnLocal(r.u16()?),
            90 => OpCode::ReturnBinaryLocalLocal {
                op: self.binary_op()?,
                left: self.reader.u16()?,
                right: self.reader.u16()?,
            },
            91 => OpCode::ReturnBinaryLocalConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.u16()?,
            },
            92 => OpCode::ReturnBinaryLocalNumberConst {
                op: self.binary_op()?,
                local: self.reader.u16()?,
                constant: self.reader.i32()?,
            },
            93 => OpCode::Return,
            94 => OpCode::Yield,
            95 => OpCode::Resume(r.u8()?),
            other => return Err(invalid(format!("unknown instruction {other}"))),
        })
    }
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
