extern crate addr2line;
extern crate fallible_iterator;
extern crate gimli;
extern crate goblin;
extern crate object;

use super::*;
use crate::symbol_table::macho::get_compact_symbol_table;
use crate::symbolicate_common::*;
use goblin::mach::{symbols, Mach};
use goblin::Object;
use serde::Serialize;
use std::collections::HashMap;
use std::mem;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Hash, Serialize)]
pub enum DebugInfoOrigin {
    ThisFile,
    OtherFile(PathBuf),
}

#[derive(Debug, Serialize)]
pub struct FoundAddressInFunction {
    pub original_address: u64,
    pub function_relative_offset: u64,
}

#[derive(Debug, Serialize)]
pub struct FunctionWithFoundAddresses {
    pub symbol_name: String,
    pub found_addresses: Vec<FoundAddressInFunction>,
}

struct OriginSection<'a> {
    file_name: &'a str,
    functions_with_found_addresses: Vec<FunctionWithFoundAddresses>,
}

enum FunctionLocation<'a> {
    OutsideOfOriginSection,
    InsidePreviousOriginSection(OriginSection<'a>),
    InsideCurrentOriginSection,
}

struct CurrentFunction<'a> {
    address: u64,
    name: &'a str,
    location: FunctionLocation<'a>,
}

struct Resolver<'a, 'b> {
    remaining_addresses_to_look_up: &'b [u64],
    current_origin_section: Option<OriginSection<'a>>,
    current_function: Option<CurrentFunction<'a>>,
    results: Vec<(DebugInfoOrigin, Vec<FunctionWithFoundAddresses>)>,
    outside_file: Vec<FunctionWithFoundAddresses>,
}

impl<'a, 'b> Resolver<'a, 'b> {
    fn new(addresses_to_look_up: &'b [u64]) -> Self {
        Resolver {
            remaining_addresses_to_look_up: addresses_to_look_up,
            current_origin_section: None,
            current_function: None,
            results: Vec::new(),
            outside_file: Vec::new(),
        }
    }

    fn split_out_addresses_before_address(&mut self, address: u64) -> &'b [u64] {
        let index = self
            .remaining_addresses_to_look_up
            .iter()
            .position(|&a| a > address)
            .unwrap_or(self.remaining_addresses_to_look_up.len());
        let (result, rest) = self.remaining_addresses_to_look_up.split_at(index);
        self.remaining_addresses_to_look_up = rest;
        result
    }

    fn enter_origin_section(&mut self, file_name: &'a str) {
        self.current_origin_section = Some(OriginSection {
            file_name,
            functions_with_found_addresses: Vec::new(),
        });
    }

    fn exit_current_origin_section(&mut self) {
        let previous_origin_section = mem::replace(&mut self.current_origin_section, None);
        if let Some(previous_origin_section) = previous_origin_section {
            if let &mut Some(ref mut current_function) = &mut self.current_function {
                if let FunctionLocation::InsideCurrentOriginSection = current_function.location {
                    current_function.location =
                        FunctionLocation::InsidePreviousOriginSection(previous_origin_section)
                }
            }
        }
    }

    fn finish_processing_function(
        &mut self,
        function: Option<CurrentFunction<'a>>,
        assigned_addresses: &[u64],
    ) {
        match function {
            Some(CurrentFunction {
                name,
                mut location,
                address,
            }) => {
                if !assigned_addresses.is_empty() {
                    let f = FunctionWithFoundAddresses {
                        symbol_name: name.to_string(),
                        found_addresses: assigned_addresses
                            .iter()
                            .map(|&a| FoundAddressInFunction {
                                original_address: a,
                                function_relative_offset: a - address,
                            })
                            .collect(),
                    };
                    match &mut location {
                        &mut FunctionLocation::OutsideOfOriginSection => {
                            self.outside_file.push(f);
                        }
                        &mut FunctionLocation::InsidePreviousOriginSection(
                            ref mut origin_section,
                        ) => {
                            origin_section.functions_with_found_addresses.push(f);
                        }
                        &mut FunctionLocation::InsideCurrentOriginSection => {
                            self.current_origin_section
                                .as_mut()
                                .unwrap()
                                .functions_with_found_addresses
                                .push(f);
                        }
                    }
                }
                if let FunctionLocation::InsidePreviousOriginSection(origin_section) = location {
                    if !origin_section.functions_with_found_addresses.is_empty() {
                        self.results.push((
                            DebugInfoOrigin::OtherFile(PathBuf::from(origin_section.file_name)),
                            origin_section.functions_with_found_addresses,
                        ));
                    }
                }
            }
            None => {
                for address in assigned_addresses {
                    println!("address {:x} is before the first FUN symbol", address);
                }
            }
        }
    }

    fn process_symbol(&mut self, (name, nlist): (&'a str, symbols::Nlist)) {
        match nlist.n_type {
            15 => {
                // FIX_ME: Find appropriate code for locally definied functions
                if name != "" {
                    let previous_function = mem::replace(
                        &mut self.current_function,
                        Some(CurrentFunction {
                            address: nlist.n_value,
                            name,
                            location: match self.current_origin_section {
                                Some(_) => FunctionLocation::InsideCurrentOriginSection,
                                None => FunctionLocation::OutsideOfOriginSection,
                            },
                        }),
                    );
                    let addresses_for_previous_function =
                        self.split_out_addresses_before_address(nlist.n_value);
                    self.finish_processing_function(
                        previous_function,
                        addresses_for_previous_function,
                    );
                }
            }
            symbols::N_OSO => {
                if name != "" {
                    self.enter_origin_section(name);
                }
            }
            symbols::N_SO => {
                if name == "" {
                    self.exit_current_origin_section();
                }
            }
            symbols::N_FUN => {
                if name != "" {
                    let previous_function = mem::replace(
                        &mut self.current_function,
                        Some(CurrentFunction {
                            address: nlist.n_value,
                            name,
                            location: match self.current_origin_section {
                                Some(_) => FunctionLocation::InsideCurrentOriginSection,
                                None => FunctionLocation::OutsideOfOriginSection,
                            },
                        }),
                    );
                    let addresses_for_previous_function =
                        self.split_out_addresses_before_address(nlist.n_value);
                    self.finish_processing_function(
                        previous_function,
                        addresses_for_previous_function,
                    );
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<(DebugInfoOrigin, Vec<FunctionWithFoundAddresses>)> {
        self.exit_current_origin_section();
        let addresses_for_last_function =
            mem::replace(&mut self.remaining_addresses_to_look_up, &[]);
        let last_function = mem::replace(&mut self.current_function, None);
        self.finish_processing_function(last_function, addresses_for_last_function);
        let mut results = self.results;
        let outside_file = self.outside_file;
        if !outside_file.is_empty() {
            results.push((DebugInfoOrigin::ThisFile, outside_file));
        }
        results
    }

    fn is_done(&self) -> bool {
        self.remaining_addresses_to_look_up.is_empty()
    }
}

pub fn resolve_to_debug_info_origins(
    lib_data: &[u8],
    addresses: &[u64],
    breakpad_id: &String,
) -> SymbolicateResult<Vec<(DebugInfoOrigin, Vec<FunctionWithFoundAddresses>)>> {
    match Object::parse(lib_data)? {
        Object::Elf(elf) => {
            println!("elf: {:#?}", &elf);
        }
        Object::PE(pe) => {
            println!("pe: {:#?}", &pe);
        }
        Object::Mach(mach) => match mach {
            Mach::Binary(mach) => {
                let mut sorted_addresses = Vec::from(addresses);
                sorted_addresses.sort();
                let mut resolver = Resolver::new(&sorted_addresses);
                for s in mach.symbols() {
                    resolver.process_symbol(s.unwrap());
                    if resolver.is_done() {
                        return Ok(resolver.finish());
                    }
                }
                return Ok(resolver.finish());
            }
            Mach::Fat(multi_arch) => {
                let mut error_msg = String::from("");
                for fat_arch in multi_arch.iter_arches().filter_map(std::result::Result::ok) {
                    println!("{:?}", fat_arch);
                    let start = fat_arch.offset as usize;
                    let end = (fat_arch.offset + fat_arch.size) as usize;
                    let address_slice = Vec::from(&lib_data[start..end]);

                    match get_compact_symbol_table(&address_slice, breakpad_id) {
                        Ok(table) => {
                            let mut result = Vec::new();
                            for addr in addresses.iter() {
                                if let Some(function_info) =
                                    get_function_info(*addr as u32, &table)?
                                {
                                    let function_offset: u64;
                                    let function_offset_str =
                                        function_info.function_offset.unwrap();
                                    if function_offset_str.starts_with("0x") {
                                        use std::i64;
                                        let without_prefix =
                                            function_offset_str.trim_start_matches("0x");
                                        function_offset =
                                            i64::from_str_radix(without_prefix, 16)? as u64;
                                    } else {
                                        function_offset = function_offset_str.parse::<u64>()?;
                                    }
                                    result.push((
                                        DebugInfoOrigin::ThisFile,
                                        vec![FunctionWithFoundAddresses {
                                            symbol_name: function_info.function.unwrap(),
                                            found_addresses: vec![FoundAddressInFunction {
                                                function_relative_offset: function_offset,
                                                original_address: *addr,
                                            }],
                                        }],
                                    ));
                                }
                            }
                            // early exit, as soon as find a correct path and table
                            return Ok(result);
                        }
                        Err(err) => {
                            error_msg = format!("{}", err.to_string());
                            log(error_msg.to_string());
                        }
                    }
                }
                return Err(SymbolicateError::InvalidInputError(error_msg));
            }
        },
        Object::Archive(archive) => {
            println!("archive: {:#?}", &archive);
        }
        Object::Unknown(magic) => println!("unknown magic: {:#x}", magic),
    }
    Ok(Vec::new())
}

pub fn resolve_to_origin_relative_addresses(
    object_file: &mut object::File,
    functions_with_addresses: Vec<FunctionWithFoundAddresses>,
) -> (Vec<u64>, Vec<FunctionInfo>) {
    use object::Object;

    let mut unlinked_module_offsets = Vec::new();
    let mut function_info_list = Vec::new();

    let mut map: HashMap<String, Vec<FoundAddressInFunction>> = functions_with_addresses
        .into_iter()
        .map(
            |FunctionWithFoundAddresses {
                 symbol_name,
                 found_addresses,
             }| (symbol_name, found_addresses),
        )
        .collect();
    for symbol in object_file.symbols() {
        if let Some(symbol_name) = symbol.name() {
            if let Some(f) = map.remove(symbol_name) {
                for FoundAddressInFunction {
                    original_address,
                    function_relative_offset,
                } in f
                {
                    function_info_list.push(FunctionInfo::new(
                        symbol_name.to_owned(),
                        function_relative_offset,
                        original_address,
                    ));
                    unlinked_module_offsets.push(symbol.address() + function_relative_offset);
                }
            }
        }
    }
    (unlinked_module_offsets, function_info_list)
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionInfo {
    pub function_name: String,
    pub function_offset: u64, // in original module
    pub module_offset: u64,   // in original module
    pub inline_info: Option<InlineStackFrameInfo>,
    pub inline_frames: Option<Vec<InlineStackFrame>>,
}

impl FunctionInfo {
    pub fn new(function_name: String, function_offset: u64, module_offset: u64) -> Self {
        FunctionInfo {
            function_name: function_name,
            function_offset: function_offset,
            module_offset: module_offset,
            inline_info: None,
            inline_frames: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineStackFrame {
    pub function_name: Option<String>,
    pub file_path: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineStackFrameInfo {
    file_path: Option<String>,
    line_number: Option<u64>,
    module_offset_in_unlinked_file: u64,
}

impl InlineStackFrameInfo {
    pub fn new(module_offset_in_unlinked_file: u64) -> Self {
        InlineStackFrameInfo {
            file_path: None,
            line_number: None,
            module_offset_in_unlinked_file: module_offset_in_unlinked_file,
        }
    }
}

fn convert_stack_frame<R: gimli::Reader>(frame: &addr2line::Frame<R>) -> InlineStackFrame {
    InlineStackFrame {
        function_name: frame
            .function
            .as_ref()
            .and_then(|f| f.demangle().ok().map(|n| n.into_owned())),
        file_path: frame.location.as_ref().and_then(|x| x.file.clone()),
        line: frame.location.as_ref().and_then(|l| l.line),
        column: frame.location.as_ref().and_then(|l| l.column),
    }
}

pub async fn resolve_all_stack_frames<'a>(
    origins: Vec<(DebugInfoOrigin, Vec<FunctionWithFoundAddresses>)>,
    read_buffer_callback: &'a js_sys::Function,
) -> SymbolicateResult<HashMap<u64, FunctionInfo>> {
    use fallible_iterator::FallibleIterator;
    let mut result = HashMap::new();
    for (origin, functions_with_found_addresses) in origins {
        match origin {
            DebugInfoOrigin::ThisFile => {
                for function_with_found_addr in functions_with_found_addresses.iter() {
                    for found_addr in function_with_found_addr.found_addresses.iter() {
                        result.insert(
                            found_addr.original_address,
                            FunctionInfo::new(
                                function_with_found_addr.symbol_name.to_string(),
                                found_addr.function_relative_offset,
                                found_addr.original_address,
                            ),
                        );
                    }
                }
            }
            DebugInfoOrigin::OtherFile(file_path) => {
                let this = JsValue::NULL;
                let path = file_path.into_os_string().into_string().unwrap();
                let buffer_future = JsFuture::from(Promise::from(
                    read_buffer_callback
                        .call1(&this, &JsValue::from(&path))
                        .or_else(|_| Err(SymbolicateError::CallbackError))?,
                ));
                let symbol_table_buffers = buffer_future.await?.dyn_into::<BuffersWrapper>()?;
                let data: Vec<u8> = symbol_table_buffers.getInnerBinaryData().buffer;
                let mut object_file = object::File::parse(&data)?;
                let (other_module_offsets, mut function_info_list) =
                    resolve_to_origin_relative_addresses(
                        &mut object_file,
                        functions_with_found_addresses,
                    );
                let context = addr2line::Context::new(&object_file)?;
                for (index, module_offset_in_unlinked_file) in
                    other_module_offsets.into_iter().enumerate()
                {
                    let mut function_info = unsafe {
                        mem::replace(&mut function_info_list[index], mem::uninitialized())
                    };
                    let mut frame_info = InlineStackFrameInfo::new(module_offset_in_unlinked_file);
                    match context.find_frames(module_offset_in_unlinked_file) {
                        Ok(frame_iter) => {
                            let frames = frame_iter
                                .map(|x| {
                                    let stack_frame = convert_stack_frame(&x);
                                    // since not all stack_frames will have line number and file_path info
                                    // we cache it as soon as we see a non-empty field
                                    if (&frame_info.file_path).is_none()
                                        && stack_frame.file_path.is_some()
                                    {
                                        frame_info.file_path = stack_frame.file_path.clone();
                                    }
                                    if (&frame_info.line_number).is_none()
                                        && stack_frame.line.is_some()
                                    {
                                        frame_info.line_number = stack_frame.line.clone();
                                    }

                                    stack_frame
                                })
                                .collect()
                                .unwrap();
                            function_info.inline_frames = Some(frames);
                            function_info.inline_info = Some(frame_info);
                            result.insert(function_info.module_offset, function_info);
                        }
                        Err(error) => {
                            log(format!("context.find_frames did not find anything for address in the original (before linking) file {:x} because of error {:?}", frame_info.module_offset_in_unlinked_file, error));
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

pub async fn get_all_stack_frames<'a>(
    file_path: &'a String,
    read_buffer_callback: &'a js_sys::Function,
    list_of_addresses: &'a Vec<u64>,
    breakpad_id: String,
) -> SymbolicateResult<HashMap<u64, FunctionInfo>> {
    let this = JsValue::NULL;
    let buffer_future = JsFuture::from(Promise::from(
        read_buffer_callback
            .call1(&this, &JsValue::from(file_path))
            .or_else(|_| Err(SymbolicateError::CallbackError))?,
    ));
    let file_buffer = buffer_future.await?.dyn_into::<BuffersWrapper>()?;
    let data: Vec<u8> = file_buffer.getInnerBinaryData().buffer;
    let result = resolve_to_debug_info_origins(&data, list_of_addresses, &breakpad_id)?;
    resolve_all_stack_frames(result, read_buffer_callback).await
}
