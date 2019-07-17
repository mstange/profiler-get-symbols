use super::*;
use crate::v6::symbolicate_linkage_resolver::get_all_stack_frames;
use response::*;

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

///  Return a list of response stacks for the module indicated by the `module_index` on memory map (in the request).
///  Each of the response stack corresponds to an address in the `addresses`.
///  parameter
/// * `module_index` - refers to the index of the specific module in the request stacks
/// * `addresses` - all the request addresses to be processed for this module
/// * `symbolicate_job` - the request job
/// * `all_candidate_paths_de` - deserialized json containing all candidate paths
/// * `read_buffer_callback` - JS callback for reading buffer
async fn process_by_module<'a>(
    module_index: usize,
    addresses: &'a Vec<u32>,
    symbolicate_job: &'a SymbolicateJob,
    all_candidate_paths_de: &'a JsonValue,
    read_buffer_callback: &'a js_sys::Function,
) -> SymbolicateResult<Vec<SymbolicateResponseStack>> {
    let mut result = Vec::new();
    let candidate_paths = get_candidate_paths_for_module(
        &all_candidate_paths_de,
        &symbolicate_job.memory_map[module_index].symbol_file_name,
    )?;
    let addresses_u64: Vec<u64> = addresses.iter().map(|x| *x as u64).collect();
    let mut error_msg = String::from("");
    for path_obj in candidate_paths {
        match get_all_stack_frames(
            &path_obj.path,
            &read_buffer_callback,
            &addresses_u64,
            symbolicate_job.get_breakpad_id(module_index)?,
        )
        .await
        {
            Ok(map) => {
                for (_, function_info) in map.into_iter() {
                    if function_info.function_name == "" {
                        break;
                    }
                    result.push(SymbolicateResponseStack {
                        module_offset: format!("{:#x}", function_info.module_offset),
                        module_name: symbolicate_job.get_module_name(module_index)?,
                        frame: module_index,
                        function_name: function_info.function_name,
                        function_offset: function_info.function_offset, // in original module
                        inline_info: function_info.inline_info,
                        inline_frames: function_info.inline_frames,
                    });
                }
                // early exit: return the result as long as a candidatePath is correct
                if !result.is_empty() {
                    return Ok(result);
                }
            }
            Err(err) => {
                error_msg = format!("{} + {}", error_msg, &err.to_string());
            }
        }
    }
    // None of the paths worked
    Err(SymbolicateError::NotFoundCandidatePath(error_msg))
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
    // Return an error as soon as request cannot be successfully parsed
    let all_candidate_paths_de: JsonValue = all_candidate_paths.into_serde()?;
    let symbolicate_jobs: Vec<SymbolicateJob> = parse_symbolicate_request(symbolicate_request)?;

    // for storing the response from multiple jobs
    let mut symbolicate_resp_json = SymbolicateResponseJson::new();

    for (_, symbolicate_job) in symbolicate_jobs.iter().enumerate() {
        // Instantiate the handler with  get_number_modules()
        // Right now the handler will be responsible PER JOB (PER RESULT),
        // Later we can consider having a larger wrapper to wrap the JobHandler class
        let mut symbolicate_job_resp = SymbolicateResponseResult::new();

        // Chunk by stack requests for the same module, process them together
        // Benefit is to read binary only once, while process all stack request for this module
        let stack_indexes_partitioned_by_module = partition_by_module_index(&symbolicate_job)?;

        for range_indices in stack_indexes_partitioned_by_module.iter() {
            let start_index = range_indices[0];
            let last_index = range_indices[1];
            let module_index = symbolicate_job.get_module_index(start_index)?;
            let module_name_id = symbolicate_job.get_module_name_id(module_index)?;
            let mut addresses = Vec::new();
            for i in start_index..last_index + 1 {
                // since last_index points to the last element of the same value,
                // to include it we must add an offset of 1
                addresses.push(symbolicate_job.get_module_offset(i)?);
            }
            match process_by_module(
                module_index,
                &addresses,
                &symbolicate_job,
                &all_candidate_paths_de,
                &read_buffer_callback,
            )
            .await
            {
                Ok(mut resp_stacks_for_the_module) => {
                    if resp_stacks_for_the_module.is_empty() {
                        for module_offset in addresses.iter() {
                            // if result is empty, push basic stacks in to the response
                            // with the size as the number of stack request for this
                            // particular module
                            resp_stacks_for_the_module.push(get_basic_response_stack(
                                *module_offset,
                                &symbolicate_job.get_module_name(module_index)?,
                                module_index,
                            ));
                        }
                        symbolicate_job_resp
                            .found_modules
                            .insert(module_name_id.to_string(), false);
                    } else {
                        symbolicate_job_resp
                            .found_modules
                            .insert(module_name_id.to_string(), true);
                    }
                    symbolicate_job_resp.push(resp_stacks_for_the_module);
                }
                Err(error) => {
                    symbolicate_job_resp
                        .errors
                        .insert(module_name_id.to_string(), error.to_string());
                    symbolicate_job_resp
                        .found_modules
                        .insert(module_name_id.to_string(), false);
                }
            }
        }
        symbolicate_resp_json.push(symbolicate_job_resp);
    }
    Ok(JsValue::from_serde(&symbolicate_resp_json.as_json())?)
}

/// Get a basic response stack by extracting provided information from the stack request
/// Hence no information about function or function_offset. This function is typically
/// called as a filler when function information is not found at a particular address
fn get_basic_response_stack(
    module_offset: u32,
    module_name: &String,
    module_index: usize,
) -> SymbolicateResponseStack {
    let mut response_stack: SymbolicateResponseStack = Default::default();
    response_stack.module_name = module_name.to_string();
    response_stack.module_offset = format!("{:#x}", module_offset); // to hex
    response_stack.frame = module_index;
    response_stack
}
