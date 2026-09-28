//! `docboss q`: jq programs, run by the jaq engine, over the JSON form of a
//! document.

use std::fmt::Write as _;

use jaq_core::data::JustLut;
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars};
use jaq_json::{Num, Val};
use serde_json::Value;

/// A compiled jq program, ready to run over any number of inputs.
pub struct Program {
    filter: jaq_core::Filter<JustLut<Val>>,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program").finish_non_exhaustive()
    }
}

/// Compiles `code` against the jq standard library, reporting lex/parse/
/// compile errors with byte positions.
pub fn compile_program(code: &str) -> Result<Program, String> {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(&arena, File { path: (), code })
        .map_err(|errors| describe_load_errors(code, errors))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| describe_compile_errors(code, errors))?;
    Ok(Program { filter })
}

/// Runs the program over one input value, collecting every output in order.
/// Runtime errors (e.g. `error("boom")`) come back as `Err` items.
pub fn run_program(program: &Program, input: Value) -> Vec<Result<Value, String>> {
    let input = match serde_json::from_value::<Val>(input) {
        Ok(val) => val,
        Err(e) => return vec![Err(format!("{e}"))],
    };
    let ctx = Ctx::<JustLut<Val>>::new(&program.filter.lut, Vars::new([]));
    program
        .filter
        .id
        .run((ctx, input))
        .map(|item| {
            item.map(|val| json_value(&val))
                .map_err(|exn| match exn.get_err() {
                    Ok(error) => format!("{error}"),
                    Err(exn) => match exn.get_halt() {
                        Ok(code) => format!("halt({code})"),
                        Err(_) => "jq: filter interrupted".to_string(),
                    },
                })
        })
        .collect()
}

/// The JSON value a jaq value denotes. Floats without a JSON representation
/// (infinities, NaN) become `null`, as stock jq prints them; byte strings
/// and non-string object keys are rendered as text.
fn json_value(val: &Val) -> Value {
    match val {
        Val::Null => Value::Null,
        Val::Bool(b) => Value::Bool(*b),
        Val::Num(num) => json_number(num),
        Val::TStr(bytes) | Val::BStr(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
        Val::Arr(items) => Value::Array(items.iter().map(json_value).collect()),
        Val::Obj(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (json_key(key), json_value(value)))
                .collect(),
        ),
    }
}

fn json_key(key: &Val) -> String {
    match json_value(key) {
        Value::String(text) => text,
        other => other.to_string(),
    }
}

fn json_number(num: &Num) -> Value {
    match num {
        Num::Int(i) => Value::from(*i as i64),
        Num::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        Num::BigInt(big) => big
            .to_string()
            .parse::<serde_json::Number>()
            .map_or(Value::Null, Value::Number),
        Num::Dec(text) => text
            .parse::<serde_json::Number>()
            .map_or(Value::Null, Value::Number),
    }
}

/// Byte offset of `part` (a slice borrowed from `code`) within `code`.
fn offset_in(code: &str, part: &str) -> usize {
    (part.as_ptr() as usize).saturating_sub(code.as_ptr() as usize)
}

fn describe_load_errors(code: &str, errors: jaq_core::load::Errors<&str, ()>) -> String {
    let mut out = String::new();
    for (_, error) in errors {
        match error {
            jaq_core::load::Error::Io(items) => {
                for (path, message) in items {
                    push_error(&mut out, &format!("io error ({path}): {message}"));
                }
            }
            jaq_core::load::Error::Lex(items) => {
                for (expected, found) in items {
                    push_error(
                        &mut out,
                        &format!(
                            "lex error at byte {}: expected {}",
                            offset_in(code, found),
                            expected.as_str()
                        ),
                    );
                }
            }
            jaq_core::load::Error::Parse(items) => {
                for (expected, found) in items {
                    push_error(
                        &mut out,
                        &format!(
                            "parse error at byte {}: expected {}",
                            offset_in(code, found),
                            expected.as_str()
                        ),
                    );
                }
            }
        }
    }
    if out.is_empty() {
        out.push_str("jq: invalid program");
    }
    out
}

fn describe_compile_errors(code: &str, errors: jaq_core::compile::Errors<&str, ()>) -> String {
    let mut out = String::new();
    for (_, file_errors) in errors {
        for (found, undefined) in file_errors {
            push_error(
                &mut out,
                &format!(
                    "compile error at byte {}: undefined {}",
                    offset_in(code, found),
                    undefined.as_str()
                ),
            );
        }
    }
    if out.is_empty() {
        out.push_str("jq: invalid program");
    }
    out
}

fn push_error(out: &mut String, message: &str) {
    if !out.is_empty() {
        out.push_str("; ");
    }
    let _ = write!(out, "jq: {message}");
}
