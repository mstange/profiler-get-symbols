pub mod error;
pub mod request;
use crate::symbol_table::compact_symbol_table;
use crate::symbolicate_common::error::{Result as SymbolicateResult, SymbolicateError};

pub struct SymbolicateFunctionInfo {
    pub function: Option<String>,
    pub function_offset: Option<String>,
}

/**
 * This module is for gluing `mod symbolicate` and other modules (such as `compact_symbol_table`)
 * or Javascript callbacks together to assemble a symbolication response from a json request.
 */

/// Return the function name and function offset if found.
/// If no exact module offset is found, then the function uses the nearest offset rounded down.
///
/// - `module_offset`: Offset corresponding to the position of module in the symbolicate request array
/// - `table`: Processed CompactSymbolTable, from reading the binary buffers of the module
pub fn get_function_info(
    module_offset: u32,
    table: &compact_symbol_table::CompactSymbolTable,
) -> SymbolicateResult<Option<SymbolicateFunctionInfo>> {
    let function_start_index: usize;
    match table.addr.binary_search(&module_offset) {
        Ok(found_func_index) => {
            function_start_index = found_func_index;
        }
        // If not found, then we take the nearest rounded down index.
        // possible_index returned by binary_search is the rounded up index, so we take
        // the one below it. Edge case would be when the possible_index is already 0, meaning the element
        // is indeed not found and there does not exist a nearest smaller element
        Err(possible_func_index) => {
            if possible_func_index != 0 {
                function_start_index = possible_func_index - 1;
            } else {
                return Err(SymbolicateError::ModuleIndexOutOfBound(
                    *table.addr.first().unwrap() as usize,
                    *table.addr.last().unwrap() as usize,
                    module_offset as usize,
                ));
            }
        }
    };
    let buffer_start: u32 = table.index[function_start_index];
    let buffer_end: u32 = table.index[function_start_index + 1];
    Ok(Some(SymbolicateFunctionInfo {
        function: Some(
            std::str::from_utf8(&table.buffer[buffer_start as usize..buffer_end as usize])
                .unwrap()
                .to_string(),
        ),
        function_offset: Some(format!(
            "{:#x}",
            module_offset - table.addr[function_start_index]
        )),
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_partition_by_module_index() {
        // TODO
    }
}
