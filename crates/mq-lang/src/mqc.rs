//! `.mqc` files: saved Tarn bytecode that runs without module lookups.
//!
//! ```rust
//! let mut engine = mq_lang::DefaultEngine::default();
//! engine.load_builtin_module();
//! let bytes = engine.precompile("upcase()", &[]).unwrap().into_bytes();
//!
//! let mut engine = mq_lang::DefaultEngine::default();
//! engine.load_builtin_module();
//! let mqc = mq_lang::Mqc::try_from(bytes).unwrap();
//! let program = engine.load(&mqc).unwrap();
//! let input = mq_lang::parse_text_input("hello").unwrap();
//! let result = engine.eval_compiled(&program, input.into_iter()).unwrap();
//! assert_eq!(result, vec!["HELLO".to_string().into()].into());
//! ```
mod code;
mod compile_io;
#[cfg(test)]
mod tests;
pub(crate) mod wire;

use crate::engine::CompiledProgram;
use crate::error::runtime::RuntimeError;
use crate::io::Io;
use crate::lexer::token::{Token, TokenKind};
use crate::runtime::builtin::{self, io_context};
use crate::tarn::{self, split_program::SplitProgram};
use crate::{Engine, Ident, ModuleResolver, Position, Range, Shared, error, parse, token_alloc};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use wire::{Reader, Writer};

const MAGIC: &[u8; 4] = b"MQC\0";
const CONTAINER_VERSION: u16 = 1;
const HEADER_LEN: usize = 16;
const SECTION_HEADER_LEN: usize = 16;
pub(crate) const CHECKSUM_LEN: usize = 32;
const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;
const REQUIRED_FLAG: u16 = 1;

/// Bumped whenever saved bytecode stops being valid for the VM.
pub const MQC_VM_ABI: u32 = 1;

const META: [u8; 4] = *b"META";
const CODE: [u8; 4] = *b"CODE";
const DEPS: [u8; 4] = *b"DEPS";
const SOURCE: [u8; 4] = *b"SRC\0";

const RECOMPILE_HELP: &str = "Recompile the program from its source with this version of mq.";

/// An error from compiling to or loading a `.mqc` file.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MqcError {
    #[error("not an mq bytecode file")]
    #[diagnostic(code(mq::mqc::not_mqc))]
    NotMqc,
    #[error("malformed .mqc file: {0}")]
    #[diagnostic(code(mq::mqc::malformed), help("{}", RECOMPILE_HELP))]
    Malformed(Cow<'static, str>),
    #[error(".mqc checksum mismatch; the file is corrupted")]
    #[diagnostic(code(mq::mqc::checksum), help("{}", RECOMPILE_HELP))]
    ChecksumMismatch,
    #[error(".mqc file is {size} bytes; the limit is {limit}")]
    #[diagnostic(code(mq::mqc::too_large))]
    TooLarge { size: u64, limit: u64 },
    #[error("unsupported .mqc container version {0}")]
    #[diagnostic(code(mq::mqc::version), help("{}", RECOMPILE_HELP))]
    UnsupportedContainerVersion(u16),
    #[error("unsupported version {version} of the {tag} section")]
    #[diagnostic(code(mq::mqc::version), help("{}", RECOMPILE_HELP))]
    UnsupportedSectionVersion { tag: String, version: u16 },
    #[error("unknown required section {0}")]
    #[diagnostic(code(mq::mqc::section), help("{}", RECOMPILE_HELP))]
    UnknownRequiredSection(String),
    #[error("duplicate section {0}")]
    #[diagnostic(code(mq::mqc::section))]
    DuplicateSection(String),
    #[error("missing required section {0}")]
    #[diagnostic(code(mq::mqc::section))]
    MissingSection(&'static str),
    #[error(
        "compiled by mq {found_version} (VM ABI {found_abi}), but this is mq {expected_version} (VM ABI {expected_abi})"
    )]
    #[diagnostic(code(mq::mqc::abi), help("{}", RECOMPILE_HELP))]
    IncompatibleVm {
        found_version: String,
        found_abi: u32,
        expected_version: String,
        expected_abi: u32,
    },
    #[error("requires functions this build does not provide: {}", .0.join(", "))]
    #[diagnostic(code(mq::mqc::features), help("Run it with an mq build that has the same features."))]
    MissingBuiltins(Vec<String>),
    #[error("cannot save {0} in a .mqc file")]
    #[diagnostic(code(mq::mqc::unsupported_value))]
    UnsupportedValue(String),
    #[error("${0} would be read at compile time and saved in the program")]
    #[diagnostic(
        code(mq::mqc::env),
        help(
            "Environment variables are read when the program runs only through string interpolation (\"${{${0}}}\") outside module-level `let`s."
        )
    )]
    EnvironmentAtCompileTime(String),
    #[error("invalid bytecode: {0}")]
    #[diagnostic(code(mq::mqc::invalid_bytecode), help("{}", RECOMPILE_HELP))]
    InvalidBytecode(String),
    #[error("\"{name}\" is not defined when module-level `let`s are computed")]
    #[diagnostic(
        code(mq::mqc::module_let_runtime_value),
        help(
            "Module-level `let` values are computed once, when the .mqc file is compiled, so they cannot read --args/--argjson values. A module can't see them, so read \"{name}\" outside the module instead."
        )
    )]
    ModuleLevelNotDefined {
        name: String,
        #[source]
        source: Box<error::Error>,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    Compile(Box<error::Error>),
}

/// A module compiled into a `.mqc` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqcDependency {
    /// The module name used in diagnostics (e.g. `csv`).
    pub name: String,
    /// The path given to `import`/`include`.
    pub specifier: String,
    /// Where the source came from (a file path or URL).
    pub origin: String,
    /// Hex SHA-256 of the module source.
    pub sha256: String,
}

/// A checked `.mqc` file. [`Engine::load`] verifies its bytecode.
#[derive(Clone)]
pub struct Mqc {
    bytes: Vec<u8>,
    meta: Meta,
    dependencies: Vec<MqcDependency>,
}

impl TryFrom<Vec<u8>> for Mqc {
    type Error = MqcError;

    /// Checks untrusted `.mqc` bytes.
    fn try_from(bytes: Vec<u8>) -> Result<Self, MqcError> {
        let sections = read_container(&bytes)?;
        let meta = read_compatible_meta(&sections)?;
        let dependencies = decode_deps(section_payload(&sections, DEPS, "DEPS")?)?;
        Ok(Self {
            bytes,
            meta,
            dependencies,
        })
    }
}

impl Mqc {
    /// Returns a metadata value stored at compile time.
    pub fn metadata(&self, key: &str) -> Option<&str> {
        find_metadata(&self.meta.metadata, key)
    }

    /// Modules compiled into the program.
    pub fn dependencies(&self) -> &[MqcDependency] {
        &self.dependencies
    }

    /// Engine globals the program reads at run time (e.g. `--args` values).
    pub fn external_globals(&self) -> &[String] {
        &self.meta.external_globals
    }

    /// The encoded file.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The encoded file.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The SHA-256 checksum at the end of the file.
    fn checksum(&self) -> [u8; CHECKSUM_LEN] {
        self.bytes[self.bytes.len() - CHECKSUM_LEN..]
            .try_into()
            .expect("checked by `Mqc::try_from`")
    }
}

impl std::fmt::Debug for Mqc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mqc")
            .field("len", &self.bytes.len())
            .field("metadata", &self.meta.metadata)
            .finish()
    }
}

#[derive(Clone)]
struct Meta {
    vm_abi: u32,
    mq_version: String,
    required_builtins: Vec<String>,
    external_globals: Vec<String>,
    metadata: Vec<(String, String)>,
}

struct SourceFile {
    name: String,
    /// `None` for modules bundled in the binary.
    text: Option<String>,
}

struct Span {
    file: u32,
    range: Range,
}

struct Section<'a> {
    tag: [u8; 4],
    version: u16,
    required: bool,
    payload: Cow<'a, [u8]>,
}

impl<T: ModuleResolver, IO: Io> Engine<T, IO> {
    /// Compiles `code` into a `.mqc` file without evaluating any input.
    ///
    /// Modules are resolved now and compiled in, and module-level `let` values are computed
    /// once and saved. Engine globals are not saved: the program reads them when it runs.
    /// `metadata` is stored as-is for the embedder (see [`Mqc::metadata`]).
    pub fn precompile(&mut self, code: &str, metadata: &[(&str, &str)]) -> Result<Mqc, MqcError> {
        let io = Shared::new(compile_io::CompileTimeIo::new(
            Shared::clone(&self.vm.io) as Shared<dyn Io>
        ));
        let result = {
            let _io_guard = io_context::scoped(Shared::clone(&io) as Shared<dyn Io>);
            self.precompile_inner(code, metadata)
        };
        // A read may also fail silently inside `try`, so check even on success.
        match io.env_read() {
            Some(name) => Err(MqcError::EnvironmentAtCompileTime(name)),
            None => result,
        }
    }

    fn precompile_inner(&mut self, code: &str, metadata: &[(&str, &str)]) -> Result<Mqc, MqcError> {
        let program = parse(code, Shared::clone(&self.token_arena)).map_err(MqcError::Compile)?;
        let prepared = tarn::build_program(&program, Shared::clone(&self.token_arena), &self.vm_module_prelude)
            .map_err(MqcError::Compile)?;
        let program = prepared.as_ref().unwrap_or(&program);

        #[cfg(feature = "sync")]
        let host_functions = self.vm.host_functions.read().unwrap().clone();
        #[cfg(not(feature = "sync"))]
        let host_functions = self.vm.host_functions.borrow().clone();
        let mut context = tarn::EngineRunContext {
            host_functions: &host_functions,
            timeout: self.vm.options.timeout,
            max_call_stack_depth: self.vm.options.max_call_stack_depth,
            capture_stack_trace: false,
            token_arena: Shared::clone(&self.token_arena),
            module_loader: self.vm.module_loader.with_same_resolver(),
            // Globals are read at run time, never baked into module values.
            global_bindings: &[],
            session: None,
            preresolved_module_vars: Default::default(),
        };
        let split = SplitProgram::compile_standalone(program, &mut context).map_err(|error| {
            let inner = error.into_inner_error(Shared::clone(&self.token_arena));
            // Only module-level `let`s are evaluated here; other unresolved names defer to run time.
            let not_defined_name = match &inner {
                error::InnerError::Runtime(
                    RuntimeError::NotDefined(_, name, _) | RuntimeError::UndefinedReference(_, name, _),
                ) => Some(name.clone()),
                _ => None,
            };
            let source = Box::new(error::Error::from_error(code, inner, self.vm.module_loader.clone()));
            match not_defined_name {
                Some(name) => MqcError::ModuleLevelNotDefined { name, source },
                None => MqcError::Compile(source),
            }
        })?;
        let encoded = code::encode(&split)?;

        let (files, spans) = self.source_table(code, &encoded.tokens, &context.module_loader)?;
        let dependencies = context
            .module_loader
            .resolved_modules()
            .into_iter()
            .map(|(name, specifier, source)| MqcDependency {
                name: name.to_string(),
                specifier: specifier.to_string(),
                origin: context
                    .module_loader
                    .get_module_path(specifier)
                    .unwrap_or_else(|_| specifier.to_string()),
                sha256: hex(&Sha256::digest(source.as_bytes())),
            })
            .collect::<Vec<_>>();
        let meta = Meta {
            vm_abi: MQC_VM_ABI,
            mq_version: env!("CARGO_PKG_VERSION").to_string(),
            required_builtins: encoded.builtins.iter().map(Ident::as_str).collect(),
            external_globals: encoded.external_globals.iter().map(Ident::as_str).collect(),
            metadata: metadata
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        };

        let bytes = write_container(&[
            section(META, encode_meta(&meta)?),
            section(CODE, encoded.payload),
            section(DEPS, encode_deps(&dependencies)?),
            section(SOURCE, encode_source(&files, &spans)?),
        ])?;
        Ok(Mqc {
            bytes,
            meta,
            dependencies,
        })
    }

    /// Loads a `.mqc` file produced by [`Engine::precompile`].
    ///
    /// The program needs no module resolution or network access. Its bytecode is verified
    /// before anything runs.
    pub fn load(&mut self, mqc: &Mqc) -> Result<CompiledProgram, MqcError> {
        let checksum = mqc.checksum();
        if let Some((cached, program)) = &self.last_mqc
            && *cached == checksum
        {
            return Ok(program.clone());
        }
        let program = self.decode_mqc(mqc)?;
        self.last_mqc = Some((checksum, program.clone()));
        Ok(program)
    }

    fn decode_mqc(&mut self, mqc: &Mqc) -> Result<CompiledProgram, MqcError> {
        // Checked by `Mqc::try_from`.
        let sections = read_sections(&mqc.bytes)?;
        let payload = |tag: [u8; 4], name: &'static str| section_payload(&sections, tag, name);
        self.check_builtins(&mqc.meta.required_builtins)?;
        let (files, spans) = decode_source(payload(SOURCE, "SOURCE")?)?;

        let module_ids = files
            .iter()
            .map(|file| {
                let text = match (&file.text, file.name.as_str()) {
                    (Some(text), _) => text.clone(),
                    (None, crate::Module::BUILTIN_MODULE) => String::new(),
                    (None, name) => standard_module_source(name)
                        .ok_or_else(|| MqcError::Malformed(format!("no source for module {name}").into()))?
                        .to_string(),
                };
                Ok(self.vm.module_loader.register_module_source(&file.name, text))
            })
            .collect::<Result<Vec<_>, MqcError>>()?;
        let tokens = spans
            .iter()
            .map(|span| {
                token_alloc(
                    &self.token_arena,
                    &Shared::new(Token {
                        range: span.range,
                        kind: TokenKind::Eof,
                        module_id: module_ids[span.file as usize],
                    }),
                )
            })
            .collect::<Vec<_>>();
        let split = code::decode(payload(CODE, "CODE")?, &tokens, Shared::clone(&self.token_arena))?;

        let source = files
            .into_iter()
            .find(|file| file.name == crate::Module::TOP_LEVEL_MODULE)
            .and_then(|file| file.text)
            .unwrap_or_default();
        Ok(CompiledProgram::from_precompiled(source, split))
    }

    fn check_builtins(&self, names: &[String]) -> Result<(), MqcError> {
        #[cfg(feature = "sync")]
        let host_functions = self.vm.host_functions.read().unwrap();
        #[cfg(not(feature = "sync"))]
        let host_functions = self.vm.host_functions.borrow();
        let missing = names
            .iter()
            .filter(|name| {
                let ident = Ident::new(name);
                builtin::get_builtin_functions(&ident).is_none() && host_functions.get(&ident).is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(MqcError::MissingBuiltins(missing))
        }
    }

    fn source_table(
        &self,
        code: &str,
        tokens: &[crate::ast::TokenId],
        module_loader: &crate::ModuleLoader<T>,
    ) -> Result<(Vec<SourceFile>, Vec<Span>), MqcError> {
        let mut files: Vec<SourceFile> = Vec::new();
        let mut file_ids: Vec<(crate::ModuleId, u32)> = Vec::new();
        let mut spans = Vec::with_capacity(tokens.len());
        for token_id in tokens {
            let token = crate::get_token(Shared::clone(&self.token_arena), *token_id);
            let file = match file_ids.iter().find(|(id, _)| *id == token.module_id) {
                Some((_, file)) => *file,
                None => {
                    let name = module_loader.module_name(token.module_id).into_owned();
                    let text = match name.as_str() {
                        crate::Module::BUILTIN_MODULE => None,
                        _ => {
                            let text = module_loader
                                .get_source_code(token.module_id, code.to_string())
                                .map_err(|error| {
                                    MqcError::Compile(Box::new(error::Error::from_error(
                                        code,
                                        error.into(),
                                        module_loader.clone(),
                                    )))
                                })?;
                            (standard_module_source(&name) != Some(text.as_str())).then_some(text)
                        }
                    };
                    let file = files.len() as u32;
                    files.push(SourceFile { name, text });
                    file_ids.push((token.module_id, file));
                    file
                }
            };
            spans.push(Span {
                file,
                range: token.range,
            });
        }
        Ok((files, spans))
    }
}

fn find_metadata<'a>(metadata: &'a [(String, String)], key: &str) -> Option<&'a str> {
    metadata
        .iter()
        .find_map(|(name, value)| (name == key).then_some(value.as_str()))
}

fn section_payload<'a>(sections: &'a [Section<'_>], tag: [u8; 4], name: &'static str) -> Result<&'a [u8], MqcError> {
    sections
        .iter()
        .find(|section| section.tag == tag)
        .map(|section| section.payload.as_ref())
        .ok_or(MqcError::MissingSection(name))
}

fn read_compatible_meta(sections: &[Section<'_>]) -> Result<Meta, MqcError> {
    let meta = decode_meta(section_payload(sections, META, "META")?)?;
    if meta.vm_abi != MQC_VM_ABI || meta.mq_version != env!("CARGO_PKG_VERSION") {
        return Err(MqcError::IncompatibleVm {
            found_version: meta.mq_version,
            found_abi: meta.vm_abi,
            expected_version: env!("CARGO_PKG_VERSION").to_string(),
            expected_abi: MQC_VM_ABI,
        });
    }
    Ok(meta)
}

/// Source of a standard module bundled in this binary.
fn standard_module_source(name: &str) -> Option<&'static str> {
    crate::STANDARD_MODULES.get(name).map(|source| source())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn section(tag: [u8; 4], payload: Vec<u8>) -> Section<'static> {
    Section {
        tag,
        version: 1,
        required: true,
        payload: Cow::Owned(payload),
    }
}

fn tag_name(tag: &[u8; 4]) -> String {
    String::from_utf8_lossy(tag).trim_end_matches('\0').to_string()
}

fn write_container(sections: &[Section<'_>]) -> Result<Vec<u8>, MqcError> {
    let body_len: usize = sections
        .iter()
        .map(|section| SECTION_HEADER_LEN + section.payload.len())
        .sum();
    let total_len = (HEADER_LEN + body_len + CHECKSUM_LEN) as u64;
    if total_len > MAX_FILE_SIZE {
        return Err(MqcError::TooLarge {
            size: total_len,
            limit: MAX_FILE_SIZE,
        });
    }
    let mut writer = Writer::default();
    writer.raw(MAGIC);
    writer.u16(CONTAINER_VERSION);
    writer.u16(HEADER_LEN as u16);
    writer.u64(total_len);
    for section in sections {
        writer.raw(&section.tag);
        writer.u16(section.version);
        writer.u16(if section.required { REQUIRED_FLAG } else { 0 });
        writer.u64(section.payload.len() as u64);
        writer.raw(&section.payload);
    }
    let mut bytes = writer.into_bytes();
    let checksum = Sha256::digest(&bytes);
    bytes.extend_from_slice(&checksum);
    Ok(bytes)
}

fn read_container(bytes: &[u8]) -> Result<Vec<Section<'_>>, MqcError> {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Err(MqcError::NotMqc);
    }
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(MqcError::TooLarge {
            size: bytes.len() as u64,
            limit: MAX_FILE_SIZE,
        });
    }
    if bytes.len() < HEADER_LEN + CHECKSUM_LEN {
        return Err(MqcError::Malformed("file is truncated".into()));
    }
    // Skips the version and header length.
    let total_len = Reader::new(&bytes[MAGIC.len() + 4..HEADER_LEN]).u64()?;
    if total_len != bytes.len() as u64 {
        return Err(MqcError::Malformed("file length does not match its header".into()));
    }
    let (content, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
    if Sha256::digest(content).as_slice() != checksum {
        return Err(MqcError::ChecksumMismatch);
    }
    read_sections(bytes)
}

/// Skips the checksum; the caller has checked it.
fn read_sections(bytes: &[u8]) -> Result<Vec<Section<'_>>, MqcError> {
    let content = &bytes[..bytes.len() - CHECKSUM_LEN];
    let mut header = Reader::new(&bytes[MAGIC.len()..HEADER_LEN]);
    let version = header.u16()?;
    let header_len = header.u16()? as usize;
    if version != CONTAINER_VERSION {
        return Err(MqcError::UnsupportedContainerVersion(version));
    }
    if header_len < HEADER_LEN || header_len > content.len() {
        return Err(MqcError::Malformed("invalid header length".into()));
    }

    let mut reader = Reader::new(&content[header_len..]);
    let mut sections: Vec<Section<'_>> = Vec::new();
    while reader.remaining() > 0 {
        let tag: [u8; 4] = reader.take(4)?.try_into().expect("four bytes");
        let version = reader.u16()?;
        let flags = reader.u16()?;
        let len = usize::try_from(reader.u64()?).map_err(|_| MqcError::Malformed("section is too large".into()))?;
        let payload = reader.take(len)?;
        if sections.iter().any(|section| section.tag == tag) {
            return Err(MqcError::DuplicateSection(tag_name(&tag)));
        }
        let required = flags & REQUIRED_FLAG != 0;
        if ![META, CODE, DEPS, SOURCE].contains(&tag) {
            if required {
                return Err(MqcError::UnknownRequiredSection(tag_name(&tag)));
            }
            continue;
        }
        if version != 1 {
            return Err(MqcError::UnsupportedSectionVersion {
                tag: tag_name(&tag),
                version,
            });
        }
        sections.push(Section {
            tag,
            version,
            required,
            payload: Cow::Borrowed(payload),
        });
    }
    Ok(sections)
}

fn write_strings(writer: &mut Writer, values: &[String]) -> Result<(), MqcError> {
    writer.len(values.len())?;
    values.iter().try_for_each(|value| writer.str(value))
}

fn read_strings(reader: &mut Reader<'_>) -> Result<Vec<String>, MqcError> {
    let len = reader.len(1)?;
    (0..len).map(|_| reader.string()).collect()
}

fn encode_meta(meta: &Meta) -> Result<Vec<u8>, MqcError> {
    let mut writer = Writer::default();
    writer.u32(meta.vm_abi);
    writer.str(&meta.mq_version)?;
    write_strings(&mut writer, &meta.required_builtins)?;
    write_strings(&mut writer, &meta.external_globals)?;
    writer.len(meta.metadata.len())?;
    for (key, value) in &meta.metadata {
        writer.str(key)?;
        writer.str(value)?;
    }
    Ok(writer.into_bytes())
}

fn decode_meta(payload: &[u8]) -> Result<Meta, MqcError> {
    let mut reader = Reader::new(payload);
    let vm_abi = reader.u32()?;
    let mq_version = reader.string()?;
    let required_builtins = read_strings(&mut reader)?;
    let external_globals = read_strings(&mut reader)?;
    let len = reader.len(2)?;
    let metadata = (0..len)
        .map(|_| Ok((reader.string()?, reader.string()?)))
        .collect::<Result<Vec<_>, MqcError>>()?;
    reader.finish("the META section")?;
    Ok(Meta {
        vm_abi,
        mq_version,
        required_builtins,
        external_globals,
        metadata,
    })
}

fn encode_deps(dependencies: &[MqcDependency]) -> Result<Vec<u8>, MqcError> {
    let mut writer = Writer::default();
    writer.len(dependencies.len())?;
    for dependency in dependencies {
        writer.str(&dependency.name)?;
        writer.str(&dependency.specifier)?;
        writer.str(&dependency.origin)?;
        writer.str(&dependency.sha256)?;
    }
    Ok(writer.into_bytes())
}

fn decode_deps(payload: &[u8]) -> Result<Vec<MqcDependency>, MqcError> {
    let mut reader = Reader::new(payload);
    let len = reader.len(4)?;
    let dependencies = (0..len)
        .map(|_| {
            Ok(MqcDependency {
                name: reader.string()?,
                specifier: reader.string()?,
                origin: reader.string()?,
                sha256: reader.string()?,
            })
        })
        .collect::<Result<Vec<_>, MqcError>>()?;
    reader.finish("the DEPS section")?;
    Ok(dependencies)
}

fn encode_source(files: &[SourceFile], spans: &[Span]) -> Result<Vec<u8>, MqcError> {
    let mut writer = Writer::default();
    writer.len(files.len())?;
    for file in files {
        writer.str(&file.name)?;
        match &file.text {
            Some(text) => {
                writer.bool(true);
                writer.str(text)?;
            }
            None => writer.bool(false),
        }
    }
    writer.len(spans.len())?;
    for span in spans {
        writer.var_u32(span.file);
        for position in [span.range.start, span.range.end] {
            writer.var_u32(position.line);
            writer.var_u32(u32::try_from(position.column).unwrap_or(u32::MAX));
        }
    }
    Ok(writer.into_bytes())
}

fn decode_source(payload: &[u8]) -> Result<(Vec<SourceFile>, Vec<Span>), MqcError> {
    let mut reader = Reader::new(payload);
    let file_count = reader.len(2)?;
    let files = (0..file_count)
        .map(|_| {
            let name = reader.string()?;
            let text = if reader.bool()? { Some(reader.string()?) } else { None };
            Ok(SourceFile { name, text })
        })
        .collect::<Result<Vec<_>, MqcError>>()?;
    let span_count = reader.len(5)?;
    let mut spans = Vec::with_capacity(span_count.min(code::MAX_PREALLOCATED));
    for _ in 0..span_count {
        let file = reader.var_u32()?;
        if file as usize >= files.len() {
            return Err(MqcError::Malformed("source span refers to a missing file".into()));
        }
        let mut position = || -> Result<Position, MqcError> {
            Ok(Position {
                line: reader.var_u32()?,
                column: reader.var_u32()? as usize,
            })
        };
        let start = position()?;
        let end = position()?;
        spans.push(Span {
            file,
            range: Range { start, end },
        });
    }
    reader.finish("the SOURCE section")?;
    Ok((files, spans))
}
