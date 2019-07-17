pub mod compact_symbol_table;
pub mod elf;
pub mod error;
pub mod macho;
pub mod pdb;
use super::*;
use crate::symbol_table::compact_symbol_table::get_compact_symbol_table_impl;
pub use crate::symbol_table::error::Result;
use crate::symbol_table::error::*;

pub fn get_compact_symbol_table(
    binary_data: &WasmMemBuffer,
    debug_data: &WasmMemBuffer,
    breakpad_id: &str,
) -> std::result::Result<CompactSymbolTable, JsValue> {
    match get_compact_symbol_table_impl(&binary_data.buffer, &debug_data.buffer, breakpad_id) {
        Result::Ok(table) => Ok(CompactSymbolTable {
            addr: table.addr,
            index: table.index,
            buffer: table.buffer,
        }),
        Result::Err(err) => {
            let error_type = GetSymbolsErrorJson::from_error(err);
            Err(JsValue::from_serde(&error_type).unwrap())
        }
    }
}
