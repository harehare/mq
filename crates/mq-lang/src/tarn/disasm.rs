//! Renders compiled Tarn bytecode as human-readable text for `mq --dump-bytecode` and tests.
use super::nodes_split::{program_after_nodes, split_at_nodes};
use super::{Error, bytecode, compiler};
use crate::TokenArena;
use crate::ast::Program;
use crate::get_token;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{ModuleLoader, ModuleResolver, Shared};
use std::fmt::Write as _;

/// Compiles a program exactly as the Engine VM path would and renders its bytecode for diagnosis.
pub(crate) fn dump_bytecode<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    global_bindings: &[(crate::Ident, RuntimeValue)],
) -> Result<String, Error> {
    let global_names: Vec<crate::Ident> = global_bindings.iter().map(|(ident, _)| *ident).collect();
    let mut output = String::new();

    let Some((before, after)) = split_at_nodes(program) else {
        let compiled =
            compiler::compile_program_for_engine(program, token_arena.clone(), module_loader, &global_names)?;
        format_compiled_bytecode(&mut output, "main", &compiled, &token_arena);
        return Ok(output);
    };

    let input_compiled = compiler::compile_program_for_engine(
        &before.to_vec(),
        token_arena.clone(),
        module_loader.clone(),
        &global_names,
    )?;
    format_compiled_bytecode(&mut output, "per-input", &input_compiled, &token_arena);

    let aggregate_compiled = compiler::compile_program_for_engine(
        &program_after_nodes(before, after),
        token_arena.clone(),
        module_loader,
        &global_names,
    )?;
    output.push('\n');
    format_compiled_bytecode(&mut output, "nodes aggregate", &aggregate_compiled, &token_arena);
    Ok(output)
}

fn format_compiled_bytecode(
    output: &mut String,
    phase: &str,
    compiled: &compiler::CompiledProgram,
    token_arena: &TokenArena,
) {
    let _ = writeln!(output, "Tarn VM bytecode");
    let _ = writeln!(output, "  phase: {phase}");
    let _ = writeln!(output, "  chunks: {}", compiled.chunks.len());
    for (chunk_index, chunk) in compiled.chunks.iter().enumerate() {
        let _ = writeln!(output, "\nChunk {chunk_index}");
        let _ = writeln!(output, "  frame");
        let _ = writeln!(
            output,
            "    local slots: {} ({})",
            chunk.local_count,
            format_slot_names(&chunk.local_names)
        );
        let _ = writeln!(
            output,
            "    upvalues: {} ({})",
            chunk.upvalue_names.len(),
            format_slot_names(&chunk.upvalue_names)
        );
        let _ = writeln!(output, "  instructions");
        for (pc, opcode) in chunk.code.iter().enumerate() {
            let location = chunk.token_at(pc).map(|token_id| {
                let token = get_token(Shared::clone(token_arena), token_id);
                format!(" @ {}:{}", token.range.start.line + 1, token.range.start.column + 1)
            });
            let _ = writeln!(
                output,
                "    {pc:04}  {}{}",
                format_opcode(opcode, chunk, pc),
                location.unwrap_or_default()
            );
        }
        if !chunk.constants.is_empty() {
            let _ = writeln!(output, "  constants");
            for (index, value) in chunk.constants.iter().enumerate() {
                let _ = writeln!(output, "    [{index}] {}", format_value(value));
            }
        }
    }
}

fn format_slot_names(names: &[crate::Ident]) -> String {
    names
        .iter()
        .enumerate()
        .map(|(slot, name)| {
            let name = name.to_string();
            if name.is_empty() {
                format!("{slot}:self")
            } else {
                format!("{slot}:{name}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn slot_ref(names: &[crate::Ident], slot: u16) -> String {
    match names.get(slot as usize) {
        Some(name) => {
            let name = name.to_string();
            if name.is_empty() {
                format!("{slot} (self)")
            } else {
                format!("{slot} ({name})")
            }
        }
        None => slot.to_string(),
    }
}

/// Renders a relative jump offset alongside the absolute instruction it targets.
fn jump_ref(pc: usize, offset: i32) -> String {
    match bytecode::jump_target(pc, offset) {
        Some(target) => format!("{offset:+} -> {target:04}"),
        None => format!("{offset:+} -> ??"),
    }
}

fn format_opcode(opcode: &bytecode::OpCode, chunk: &bytecode::Chunk, pc: usize) -> String {
    let local = |slot: u16| slot_ref(&chunk.local_names, slot);
    let upvalue = |slot: u16| slot_ref(&chunk.upvalue_names, slot);
    match opcode {
        #[cfg(feature = "debugger")]
        bytecode::OpCode::StmtBoundary(_) => "StmtBoundary".to_string(),
        #[cfg(feature = "debugger")]
        bytecode::OpCode::Breakpoint(_) => "Breakpoint".to_string(),
        bytecode::OpCode::Const(index) => format!("Const {index}"),
        bytecode::OpCode::PushNone => "PushNone".to_string(),
        bytecode::OpCode::GetLocal(slot) => format!("GetLocal {}", local(*slot)),
        bytecode::OpCode::SetLocal(slot) => format!("SetLocal {}", local(*slot)),
        bytecode::OpCode::TeeLocal(slot) => format!("TeeLocal {}", local(*slot)),
        bytecode::OpCode::GetUpvalue(slot) => format!("GetUpvalue {}", upvalue(*slot)),
        bytecode::OpCode::SetUpvalue(slot) => format!("SetUpvalue {}", upvalue(*slot)),
        bytecode::OpCode::MakeClosure(payload) => {
            format!("MakeClosure chunk {}, upvalues {}", payload.0, payload.1.len())
        }
        bytecode::OpCode::MakeStaticClosure(chunk) => format!("MakeStaticClosure chunk {chunk}"),
        bytecode::OpCode::Pop => "Pop".to_string(),
        bytecode::OpCode::Dup => "Dup".to_string(),
        bytecode::OpCode::Jump(offset) => format!("Jump {}", jump_ref(pc, *offset)),
        bytecode::OpCode::JumpIfFalse(offset) => format!("JumpIfFalse {}", jump_ref(pc, *offset)),
        bytecode::OpCode::Add => "Add".to_string(),
        bytecode::OpCode::Sub => "Sub".to_string(),
        bytecode::OpCode::Mul => "Mul".to_string(),
        bytecode::OpCode::Div => "Div".to_string(),
        bytecode::OpCode::Mod => "Mod".to_string(),
        bytecode::OpCode::Eq => "Eq".to_string(),
        bytecode::OpCode::Ne => "Ne".to_string(),
        bytecode::OpCode::Lt => "Lt".to_string(),
        bytecode::OpCode::Le => "Le".to_string(),
        bytecode::OpCode::Gt => "Gt".to_string(),
        bytecode::OpCode::Ge => "Ge".to_string(),
        bytecode::OpCode::BinaryLocalLocal { op, left, right } => {
            format!("BinaryLocalLocal {op:?} {}, {}", local(*left), local(*right))
        }
        bytecode::OpCode::BinaryLocalConst {
            op,
            local: slot,
            constant,
        } => {
            format!("BinaryLocalConst {op:?} {}, const {constant}", local(*slot))
        }
        bytecode::OpCode::Neg => "Neg".to_string(),
        bytecode::OpCode::Not => "Not".to_string(),
        bytecode::OpCode::ArrayNew => "ArrayNew".to_string(),
        bytecode::OpCode::ArrayPush => "ArrayPush".to_string(),
        bytecode::OpCode::ArraySpread => "ArraySpread".to_string(),
        bytecode::OpCode::DictSpread => "DictSpread".to_string(),
        bytecode::OpCode::ToForeachIterable => "ToForeachIterable".to_string(),
        bytecode::OpCode::ArrayLen => "ArrayLen".to_string(),
        bytecode::OpCode::ArrayGetAt => "ArrayGetAt".to_string(),
        bytecode::OpCode::ArrayLenLocal(slot) => format!("ArrayLenLocal {}", local(*slot)),
        bytecode::OpCode::ArrayGetLocalAt { array_slot, index_slot } => {
            format!("ArrayGetLocalAt {}, {}", local(*array_slot), local(*index_slot))
        }
        bytecode::OpCode::ForeachNext {
            array_slot,
            index_slot,
            value_slot,
            exit_offset,
        } => format!(
            "ForeachNext array {}, index {}, value {}, exit {}",
            local(*array_slot),
            local(*index_slot),
            local(*value_slot),
            jump_ref(pc, *exit_offset)
        ),
        bytecode::OpCode::ForeachCollect(slot) => format!("ForeachCollect {}", local(*slot)),
        bytecode::OpCode::ArraySliceFrom => "ArraySliceFrom".to_string(),
        bytecode::OpCode::DictGetLocalOrFail {
            subject_slot,
            key,
            value_slot,
        } => format!(
            "DictGetLocalOrFail {}, {key} -> {}",
            local(*subject_slot),
            local(*value_slot)
        ),
        bytecode::OpCode::TypeCheck(name) => format!("TypeCheck {name}"),
        bytecode::OpCode::GetEnvVar(index) => format!("GetEnvVar const {index}"),
        bytecode::OpCode::GetExternalGlobal(name) => format!("GetExternalGlobal {name}"),
        bytecode::OpCode::InterpString(count) => format!("InterpString {count}"),
        bytecode::OpCode::SelectorMatch(selector) => format!("SelectorMatch {selector:?}"),
        bytecode::OpCode::SelectorMatchKind(kind) => format!("SelectorMatchKind {kind:?}"),
        bytecode::OpCode::SelectorMatchHeading(level) => format!("SelectorMatchHeading {level}"),
        bytecode::OpCode::SelectorMatchWithArgs(payload) => {
            format!("SelectorMatchWithArgs {:?}, argc={}", payload.0, payload.1)
        }
        bytecode::OpCode::CallBuiltin(name, argc) => format!("CallBuiltin {name}, argc={argc}"),
        bytecode::OpCode::CallLocal(slot, argc) => format!("CallLocal {}, argc={argc}", local(*slot)),
        bytecode::OpCode::CallValue(argc) => format!("CallValue argc={argc}"),
        bytecode::OpCode::MaybeAutoCall => "MaybeAutoCall".to_string(),
        bytecode::OpCode::TryCatch(info) => {
            let break_acc = info.break_acc_slot.map(local).unwrap_or_else(|| "-".to_string());
            let break_target = info
                .break_offset
                .map(|offset| jump_ref(pc, offset))
                .unwrap_or_else(|| "-".to_string());
            let continue_target = info
                .continue_offset
                .map(|offset| jump_ref(pc, offset))
                .unwrap_or_else(|| "-".to_string());
            format!(
                "TryCatch has_binder={}, break_acc={break_acc}, break={break_target}, continue={continue_target}",
                info.has_binder
            )
        }
        bytecode::OpCode::FlowBreak(has_value) => format!("FlowBreak has_value={has_value}"),
        bytecode::OpCode::FlowContinue => "FlowContinue".to_string(),
        bytecode::OpCode::RaiseDestructuringFailed => "RaiseDestructuringFailed".to_string(),
        bytecode::OpCode::Return => "Return".to_string(),
    }
}

fn format_value(value: &RuntimeValue) -> String {
    const MAX_CHARS: usize = 96;

    let rendered = value.to_string();
    let mut chars = rendered.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}
