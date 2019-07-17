use super::*;
use crate::symbol_table::compact_symbol_table;
pub use crate::symbolicate_common::*;
use response::*;

async fn try_paths_and_get_compact_symbol_table<'a>(
    symbolicate_job: &'a SymbolicateJob,
    module_index: usize,
    candidate_paths: &'a Vec<CandidatePath>,
    read_buffer_callback: &'a js_sys::Function,
) -> SymbolicateResult<compact_symbol_table::CompactSymbolTable> {
    let breakpad_id = symbolicate_job.memory_map[module_index]
        .debug_id
        .to_string();
    let module_key = symbolicate_job.memory_map[module_index].as_string();

    for path_obj in candidate_paths {
        match try_path_and_fetch_buffer(&path_obj.path, &path_obj.debugPath, &read_buffer_callback)
            .await
        {
            Ok(buffer_param) => {
                // debug_data is None if path_obj.path == path_obj.debugPath
                // this is because debug_data will be equivalent to binary_data in this case
                let compact_symbol_table: compact_symbol_table::CompactSymbolTable =
                    match buffer_param.debug_data {
                        Some(debug_data) => get_compact_symbol_table_internal(
                            &buffer_param.binary_data,
                            &debug_data,
                            &breakpad_id,
                        )?,
                        None => get_compact_symbol_table_internal(
                            &buffer_param.binary_data,
                            &buffer_param.binary_data,
                            &breakpad_id,
                        )?,
                    };
                return Ok(compact_symbol_table);
            }
            Err(_) => {}
        }
    }
    return Err(SymbolicateError::NotFoundCandidatePath(module_key));
}

pub struct GetSymbolTableBuffers {
    pub binary_data: WasmMemBuffer,
    pub debug_data: Option<WasmMemBuffer>,
}

/// Asynchronously returns buffers from the given path if valid, else return Err(.)
///
/// * `path` - Candidate path of a module
/// * `debugPath` - Candidate debug path of a module
///
/// If candidate_path and debugPath are the same, they point to the same buffer by reference.
/// Else, will point to different buffers.
async fn try_path_and_fetch_buffer<'a>(
    path: &'a String,
    debugPath: &'a String,
    read_buffer_callback: &'a js_sys::Function,
    // read_buffer_callback takes in path and debugPath, returns JSBuffer
) -> SymbolicateResult<GetSymbolTableBuffers> {
    let this = JsValue::NULL;
    // need to modify the error to identify whether it's 'path not found' or etc error
    let buffer_future = JsFuture::from(Promise::from(
        read_buffer_callback
            .call2(
                &this,
                &JsValue::from(path.to_string()),
                &JsValue::from(debugPath.to_string()),
            )
            .or_else(|_| Err(SymbolicateError::CallbackError))?,
    ));
    let symbol_table_buffers = buffer_future.await?.dyn_into::<BuffersWrapper>()?;
    if path != debugPath {
        Ok(GetSymbolTableBuffers {
            binary_data: symbol_table_buffers.getInnerBinaryData(),
            debug_data: Some(symbol_table_buffers.getInnerDebugData()),
        })
    } else {
        Ok(GetSymbolTableBuffers {
            binary_data: symbol_table_buffers.getInnerBinaryData(),
            debug_data: None,
        })
    }
}

/// Returns symbolicate response or error as JsValue, wrapped in a Future object
pub async fn get_symbolicate_response_impl(
    symbolicate_request: JsValue,
    read_buffer_callback: js_sys::Function, // read_buffer_callback takes two param: fileName, debugId
    candidate_paths: JsValue,
) -> std::result::Result<JsValue, JsValue> {
    match process_symbolicate_request(
        &symbolicate_request,
        &read_buffer_callback,
        &candidate_paths,
    )
    .await
    {
        SymbolicateResult::Ok(resp) => Ok(resp),
        SymbolicateResult::Err(err) => {
            let error_type = SymbolicateErrorJson::from_error(err);
            Err(JsValue::from_serde(&error_type).unwrap())
        }
    }
}

/// Parse one or more request stacks for the same module
/// The range of stacks between `start_index` and `last_index` (inclusive)
/// all point to the same module.
///
/// Receives a job and then returns a response stack.
/// * `symbolicate_job`: The parsed symbolicateJob from the json request
/// * `module_index`: index of the module in the stacks of the symbolicate request
/// * `candidate_paths`: candidate paths specific to this module
/// * `read_buffer_callback` test a pair of candidate paths (binary and debug) for a module
///     retrieve the module in the form of binary buffer if valid, else return Err()
///
/// To assemble the resulting stack into a symbolicate job response, caller will
/// need to read the information from the stack (such as function module)
/// and insert this the stack into the response.
async fn process_by_module<'a>(
    start_index: usize,
    last_index: usize,
    symbolicate_job: &'a SymbolicateJob,
    all_candidate_paths_de: &'a JsonValue,
    read_buffer_callback: &'a js_sys::Function,
) -> SymbolicateResult<ResponseStackResult> {
    let module_index = symbolicate_job.get_module_index(start_index)?;
    let candidate_paths = get_candidate_paths_for_module(
        &all_candidate_paths_de,
        &symbolicate_job.get_module_name(module_index)?,
    )?;
    let compact_symbol_table = try_paths_and_get_compact_symbol_table(
        symbolicate_job,
        module_index,
        &candidate_paths,
        read_buffer_callback,
    )
    .await?;
    let mut is_module_found = true;
    let mut response_stacks_for_this_module = vec![];
    for i in start_index..last_index + 1 {
        let module_offset = symbolicate_job.get_module_offset(i)?; // varies by stack request

        match process_stack(
            &compact_symbol_table,
            symbolicate_job.get_module_name(module_index)?,
            module_offset,
            module_index,
        ) {
            Ok(response_stack) => {
                response_stacks_for_this_module.push(response_stack);
            }
            Err(_) => {
                // construct a basic stack for this disregarding the function information
                let mut response_stack: SymbolicateResponseStack = Default::default();
                response_stack.module = symbolicate_job.get_module_name(module_index)?;
                response_stack.module_offset = format!("{:#x}", module_offset); // to hex
                response_stack.frame = module_index;
                response_stacks_for_this_module.push(response_stack);
                is_module_found = false;
            }
        }
    }
    Ok(ResponseStackResult {
        stacks: response_stacks_for_this_module,
        is_module_found: is_module_found,
    })
}

/// Given a symbolicate request json, parse and return a promise containing
/// the symbolication response or the error. The outer "jobs" field
/// wrapper is optional. This function handles both scenarios of whether
/// the outer "jobs" field exists or not.
/// * `symbolicate_request` - symbolicate request json
/// * `read_buffer_callback` - javascript callback to read the file name from memory.
///     Takes in two parameter (candidate_path, candidate_debugPath) which are potential
///     paths of where the module is found.
/// * `candidate_paths` - an json object of all candidate paths for all the modules
///     listed in the request.
async fn process_symbolicate_request<'a>(
    symbolicate_request: &'a JsValue,
    read_buffer_callback: &'a js_sys::Function,
    all_candidate_paths: &'a JsValue,
) -> SymbolicateResult<JsValue> {
    let all_candidate_paths_de: JsonValue = all_candidate_paths.into_serde()?;
    let symbolicate_jobs: Vec<SymbolicateJob> = parse_symbolicate_request(symbolicate_request)?;

    // for storing the response from multiple jobs
    let mut symbolicate_resp_json = SymbolicateResponseJson::new();

    for (_, symbolicate_job) in symbolicate_jobs.iter().enumerate() {
        // Instantiate the handler with  get_number_modules()
        // Right now the handler will be responsible PER JOB (PER RESULT),
        // Later we can consider having a larger wrapper to wrap the JobHandler class
        let mut symbolicate_job_resp = SymbolicateResponseResult::new();

        let stack_indexes_partitioned_by_module = partition_by_module_index(&symbolicate_job)?;

        for range_indices in stack_indexes_partitioned_by_module.iter() {
            let start_index = range_indices[0];
            let last_index = range_indices[1];
            let module_index = symbolicate_job.get_module_index(start_index)?;
            let module_name_id = symbolicate_job.get_module_name_id(module_index)?;
            match process_by_module(
                start_index,
                last_index,
                &symbolicate_job,
                &all_candidate_paths_de,
                &read_buffer_callback,
            )
            .await
            {
                Ok(stack_result) => {
                    symbolicate_job_resp.push(stack_result.stacks);
                    symbolicate_job_resp
                        .found_modules
                        .insert(module_name_id, stack_result.is_module_found); // may be true or false
                }
                Err(error) => {
                    // construct a basic stack (which doesn't have function information)
                    // it means that the call didn't succeed for even one
                    let mut stacks = vec![];

                    for i in start_index..last_index + 1 {
                        let module_name = symbolicate_job.get_module_name(module_index)?;
                        let module_offset = symbolicate_job.get_module_offset(i)?;
                        let response_stack =
                            get_basic_response_stack(module_offset, module_name, module_index);
                        stacks.push(response_stack);
                    }
                    symbolicate_job_resp.push(stacks);
                    symbolicate_job_resp
                        .found_modules
                        .insert(module_name_id.to_string(), false);
                    symbolicate_job_resp
                        .errors
                        .insert(module_name_id.to_string(), error.to_string());
                }
            }
        }
        symbolicate_resp_json.push(symbolicate_job_resp);
    }
    Ok(JsValue::from_serde(&symbolicate_resp_json.as_json())?)
}

fn get_basic_response_stack(
    module_offset: u32,
    module_name: String,
    module_index: usize,
) -> SymbolicateResponseStack {
    let mut response_stack: SymbolicateResponseStack = Default::default();
    response_stack.module = module_name;
    response_stack.module_offset = format!("{:#x}", module_offset); // to hex
    response_stack.frame = module_index;
    response_stack
}

fn process_stack(
    compact_symbol_table: &compact_symbol_table::CompactSymbolTable,
    module_name: String,
    module_offset: u32,
    module_index: usize,
) -> SymbolicateResult<SymbolicateResponseStack> {
    let function_info: Option<SymbolicateFunctionInfo> =
        get_function_info(module_offset, compact_symbol_table)?;
    let mut response_stack: SymbolicateResponseStack = Default::default();
    response_stack.from(function_info.unwrap());
    response_stack.module = module_name;
    response_stack.module_offset = format!("{:#x}", module_offset); // to hex
    response_stack.frame = module_index;
    return Ok(response_stack);
}
