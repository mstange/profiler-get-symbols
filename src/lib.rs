#![feature(async_await)]
#[macro_use]
extern crate serde_json;
extern crate goblin;
extern crate js_sys;
extern crate object;
extern crate pdb as pdb_crate;
extern crate scroll;
extern crate serde;
extern crate serde_derive;
extern crate uuid;
extern crate wasm_bindgen;
extern crate wasm_bindgen_futures;
#[macro_use]
extern crate wasm_bindgen_test;

pub mod symbol_table;
pub mod symbolicate_common;
pub mod v5;
pub mod v6;
use goblin::{mach, Hint};
use js_sys::Promise;
use serde_json::Value as JsonValue;
use std::io::Cursor;
use std::mem;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
#[macro_use]
use wasm_bindgen_futures::futures_0_3::*;

#[wasm_bindgen]
extern "C" {
    pub type BuffersWrapper;
    #[wasm_bindgen(structural, method)]
    fn getInnerDebugData(this: &BuffersWrapper) -> WasmMemBuffer;
    #[wasm_bindgen(structural, method)]
    fn getInnerBinaryData(this: &BuffersWrapper) -> WasmMemBuffer;
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log(s: String);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_str(s: &str);
}

#[wasm_bindgen]
pub struct CompactSymbolTable {
    addr: Vec<u32>,
    index: Vec<u32>,
    buffer: Vec<u8>,
}

#[wasm_bindgen]
impl CompactSymbolTable {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            addr: vec![],
            index: vec![],
            buffer: vec![],
        }
    }

    pub fn take_addr(&mut self) -> Vec<u32> {
        mem::replace(&mut self.addr, vec![])
    }
    pub fn take_index(&mut self) -> Vec<u32> {
        mem::replace(&mut self.index, vec![])
    }
    pub fn take_buffer(&mut self) -> Vec<u8> {
        mem::replace(&mut self.buffer, vec![])
    }
}

/// WasmMemBuffer lets you allocate a chunk of memory on the wasm heap and
/// directly initialize it from JS without a copy. The constructor takes the
/// allocation size and a callback function which does the initialization.
/// This is useful if you need to get very large amounts of data from JS into
/// wasm (for example, the contents of a 1.7GB libxul.so).
#[wasm_bindgen]
pub struct WasmMemBuffer {
    buffer: Vec<u8>,
}

#[wasm_bindgen]
impl WasmMemBuffer {
    /// Create the buffer and initialize it synchronously in the callback function.
    /// f is called with one argument: the Uint8Array that wraps our buffer.
    /// f should not return anything; its return value is ignored.
    /// f must not call any exported wasm functions! Anything that causes the
    /// wasm heap to resize will invalidate the typed array's internal buffer!
    /// Do not hold on to the array that is passed to f after f completes.
    #[wasm_bindgen(constructor)]
    pub fn new(byte_length: u32, f: &js_sys::Function) -> Self {
        // See https://github.com/rustwasm/wasm-bindgen/issues/1643 for how
        // to improve this method.
        let mut buffer = vec![0; byte_length as usize];
        unsafe {
            // Let JavaScript fill the buffer without making a copy.
            // We give the callback function access to the wasm memory via a
            // JS Uint8Array which wraps the underlying wasm memory buffer at
            // the appropriate offset and length.
            // The callback function is supposed to mutate the contents of
            // buffer. However, the "&mut" here is a bit of a lie:
            // Uint8Array::view takes an immutable reference to a slice, not a
            // mutable one. This is rather sketchy but seems to work for now.
            // https://github.com/rustwasm/wasm-bindgen/issues/1079#issuecomment-508577627
            let array = js_sys::Uint8Array::view(&mut buffer);
            f.call1(&JsValue::NULL, &JsValue::from(array))
                .expect("The callback function should not throw");
        }
        Self { buffer }
    }
}

#[wasm_bindgen]
pub fn get_inline_frames(
    file_path: String,
    read_buffer_callback: js_sys::Function,
    list_of_addresses: Vec<u32>,
    breakpad_id: String,
) -> Promise {
    future_to_promise(crate::v6::get_all_stack_frames_impl(
        //  get_all_stack_frames_impl
        file_path,
        read_buffer_callback,
        list_of_addresses,
        breakpad_id,
    ))
}
#[wasm_bindgen]
pub fn get_compact_symbol_table(
    binary_data: &WasmMemBuffer,
    debug_data: &WasmMemBuffer,
    breakpad_id: &str,
) -> std::result::Result<CompactSymbolTable, JsValue> {
    symbol_table::get_compact_symbol_table(binary_data, debug_data, breakpad_id)
}

#[wasm_bindgen]
pub fn get_symbolicate_response(
    symbolicate_request: JsValue,
    read_buffer_callback: js_sys::Function, // read_buffer_callback takes two param: candidatePath, debugPath
    candidate_paths: JsValue,
    url: String,
) -> Promise {
    return match url.as_ref() {
        "symbolicate/v5" => {
            v5::get_symbolicate_response(symbolicate_request, read_buffer_callback, candidate_paths)
        }
        "symbolicate/v6" => {
            v6::get_symbolicate_response(symbolicate_request, read_buffer_callback, candidate_paths)
        }
        invalid_url => {
            log(format!("Invalid URL called in rust : {}", invalid_url));
            use crate::symbolicate_common::error::{SymbolicateError, SymbolicateErrorJson};
            let error_type = SymbolicateErrorJson::from_error(SymbolicateError::InvalidInputError(
                String::from("Invalid URL"),
            ));
            Promise::reject(&JsValue::from_serde(&error_type).unwrap())
        }
    };
}
