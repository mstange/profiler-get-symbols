use super::*;
use crate::WasmMemBuffer;
use object::{Object, SymbolKind};
use std::collections::HashMap;
use std::ops::Deref;

#[repr(C)]
pub struct CompactSymbolTable {
    pub addr: Vec<u32>,
    pub index: Vec<u32>,
    pub buffer: Vec<u8>,
}

impl CompactSymbolTable {
    pub fn new() -> Self {
        Self {
            addr: Vec::new(),
            index: Vec::new(),
            buffer: Vec::new(),
        }
    }

    pub fn from_map<T: Deref<Target = str>>(map: HashMap<u32, T>) -> Self {
        let mut table = Self::new();
        let mut entries: Vec<_> = map.into_iter().collect();
        entries.sort_by_key(|&(addr, _)| addr);
        for (addr, name) in entries {
            table.addr.push(addr);
            table.index.push(table.buffer.len() as u32);
            table.add_name(&name);
        }
        table.index.push(table.buffer.len() as u32);
        table
    }

    pub fn from_object<'a, 'b, T>(object_file: &'b T) -> Self
    where
        T: Object<'a, 'b>,
    {
        Self::from_map(
            object_file
                .dynamic_symbols()
                .chain(object_file.symbols())
                .filter(|symbol| symbol.kind() == SymbolKind::Text)
                .filter_map(|symbol| symbol.name().map(|name| (symbol.address() as u32, name)))
                .collect(),
        )
    }

    fn add_name(&mut self, name: &str) {
        self.buffer.extend_from_slice(name.as_bytes());
    }
}

/// For internal (non-wasm) usage, advantage is to get error message without serializing
/// and deserializing.
pub fn get_compact_symbol_table_internal(
    binary_data: &WasmMemBuffer,
    debug_data: &WasmMemBuffer,
    breakpad_id: &str,
) -> Result<CompactSymbolTable> {
    match get_compact_symbol_table_impl(&binary_data.buffer, &debug_data.buffer, breakpad_id) {
        Result::Ok(table) => Ok(CompactSymbolTable {
            addr: table.addr,
            index: table.index,
            buffer: table.buffer,
        }),
        Err(err) => Err(err),
    }
}

pub fn get_compact_symbol_table_impl(
    binary_data: &[u8],
    debug_data: &[u8],
    breakpad_id: &str,
) -> Result<compact_symbol_table::CompactSymbolTable> {
    let mut reader = Cursor::new(binary_data);
    match goblin::peek(&mut reader)? {
        Hint::Elf(_) => elf::get_compact_symbol_table(binary_data, breakpad_id),
        Hint::Mach(_) => macho::get_compact_symbol_table(binary_data, breakpad_id),
        Hint::MachFat(_) => {
            let mut first_error = None;
            let multi_arch = mach::MultiArch::new(binary_data)?;
            for fat_arch in multi_arch.iter_arches().filter_map(std::result::Result::ok) {
                let arch_slice = fat_arch.slice(binary_data);
                match macho::get_compact_symbol_table(arch_slice, breakpad_id) {
                    Ok(table) => {
                        return Ok(table);
                    }
                    Err(err) => first_error = Some(err),
                }
            }
            Err(first_error.unwrap_or_else(|| {
                GetSymbolsError::InvalidInputError("Incompatible system architecture")
            }))
        }
        Hint::PE => pdb::get_compact_symbol_table(debug_data, breakpad_id),
        _ => Err(GetSymbolsError::InvalidInputError(
            "goblin::peek fails to read",
        )),
    }
}
