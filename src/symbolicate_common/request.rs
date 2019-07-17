extern crate serde;
extern crate serde_derive;
extern crate wasm_bindgen;
use crate::symbolicate_common::error::{Result as SymbolicateResult, SymbolicateError};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct CandidatePath {
    pub path: String,
    pub debugPath: String,
}

#[derive(Serialize)]
pub struct SymbolicateJob {
    pub memory_map: Vec<SymbolicateMemoryMap>,
    pub stacks: Vec<SymbolicateRequestStack>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SymbolicateMemoryMap {
    pub symbol_file_name: String,
    pub debug_id: String,
}

#[derive(Serialize)]
pub struct SymbolicateRequestStack {
    pub module_index_and_offset: Vec<u32>,
}

impl SymbolicateJob {
    pub fn is_valid_module_index(&self, module_index: usize) -> SymbolicateResult<()> {
        if module_index > self.memory_map.len() {
            return Err(SymbolicateError::ModuleIndexOutOfBound(
                0,
                self.memory_map.len(),
                module_index,
            ));
        }
        Ok(())
    }

    pub fn is_valid_stack_index(&self, stack_index: usize) -> SymbolicateResult<()> {
        if stack_index > self.stacks.len() {
            return Err(SymbolicateError::ModuleIndexOutOfBound(
                0,
                self.stacks.len(),
                stack_index,
            ));
        }
        Ok(())
    }

    pub fn get_module_index(&self, stack_index: usize) -> SymbolicateResult<usize> {
        self.is_valid_stack_index(stack_index)?;
        Ok(self.stacks[stack_index].get_module_index() as usize)
    }
    pub fn get_module_name_id(&self, module_index: usize) -> SymbolicateResult<String> {
        self.is_valid_module_index(module_index)?;
        Ok(self.memory_map[module_index].as_string())
    }
    pub fn get_module_name(&self, module_index: usize) -> SymbolicateResult<String> {
        self.is_valid_module_index(module_index)?;
        Ok(self.memory_map[module_index].symbol_file_name.to_owned())
    }
    pub fn get_breakpad_id(&self, module_index: usize) -> SymbolicateResult<String> {
        self.is_valid_module_index(module_index)?;
        Ok(self.memory_map[module_index].debug_id.to_owned())
    }
    pub fn get_module_offset(&self, stack_index: usize) -> SymbolicateResult<u32> {
        self.is_valid_stack_index(stack_index)?;
        Ok(self.stacks[stack_index].get_module_offset())
    }
}

impl SymbolicateMemoryMap {
    pub fn as_string(&self) -> String {
        format!("{}/{}", self.symbol_file_name, self.debug_id)
    }
}

impl SymbolicateRequestStack {
    pub fn get_module_offset(&self) -> u32 {
        self.module_index_and_offset[1]
    }

    pub fn get_module_index(&self) -> u32 {
        self.module_index_and_offset[0]
    }
}

/// The "jobs" parameter wrapper is optional. In other words, can input request in
/// the format of `{"memory_map" [...], "stacks": [...]}`
///
/// * `symbolicate_request` : JSON symbolicate request
pub fn parse_symbolicate_request(
    symbolicate_request: &JsValue,
) -> SymbolicateResult<Vec<SymbolicateJob>> {
    let symbolicate_request_de: JsonValue = symbolicate_request.into_serde()?;
    let symbolicate_jobs: SymbolicateResult<Vec<SymbolicateJob>> =
        match symbolicate_request_de.get("jobs") {
            // If the json starts with a 'jobs' key (aka, with a lot of jobs)
            // deconstruct into the array by first
            Some(jobs) => jobs
                .as_array()
                .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
                .iter()
                .map(|x| parse_request_job(x))
                .collect(),
            // else, parse the job object directly
            None => Ok(vec![parse_request_job(&symbolicate_request_de)?]),
        };
    symbolicate_jobs
}

/// Deconstructs the `symbolicate_job` json object into a SymbolicateJob
pub fn parse_request_job(symbolicate_job: &JsonValue) -> SymbolicateResult<SymbolicateJob> {
    Ok(SymbolicateJob {
        memory_map: parse_request_memory_map(&symbolicate_job["memoryMap"])?,
        stacks: parse_request_stacks(&symbolicate_job["stacks"])?,
    })
}

/// Accepts a `req_stacks` as a JSON object, returns a vector of `SymbolicateRequestStack`
/// * `req_stacks` - JSON object
///
/// Example of the `i` stack in the `req_stacks` object:
/// * `stack[i][0]` - module_index: position of module in the symbolicate request `stacks`
/// * `stack[i][1]` - module_offset
/// `{
///  "stacks": [
///             [
///               [0, 11723767],
///               [1, 65802]
///             ]
///           ]
/// }1
///
/// Example of `Vec<SymbolicateRequestStack>` output:
/// vec![
///     SymbolicateRequestStack {
///         "module_index_and_offset": [0, 11723767]
///     }
/// ]
pub fn parse_request_stacks(
    req_stacks: &JsonValue,
) -> SymbolicateResult<Vec<SymbolicateRequestStack>> {
    let mut result = Vec::new();
    let req_stacks_arr = req_stacks
        .as_array()
        .ok_or_else(|| SymbolicateError::JsonParseArrayError)?;

    if req_stacks_arr.is_empty() {
        return Err(SymbolicateError::InvalidInputError(format!(
            "The given req_stacks_arr is empty"
        )));
    }

    for (_, inner_job_stack) in req_stacks_arr.iter().enumerate() {
        let inner_job_stack_arr = inner_job_stack
            .as_array()
            .ok_or_else(|| SymbolicateError::JsonParseArrayError)?;
        if inner_job_stack_arr.is_empty() {
            return Err(SymbolicateError::InvalidInputError(format!(
                "The given req_stacks_arr is empty"
            )));
        }
        for (_, stack) in inner_job_stack_arr.iter().enumerate() {
            let stack_len = stack
                .as_array()
                .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
                .len();
            if stack_len != 2 {
                return Err(SymbolicateError::InvalidInputError(format!(
                    "The length of request stack needs to be 2, but received {}",
                    stack_len
                )));
            }
            let tup_stack = SymbolicateRequestStack {
                module_index_and_offset: serde_json::from_value(stack.clone()).unwrap(),
            };
            result.push(tup_stack);
        }
    }
    result.sort_by_key(|stack| stack.module_index_and_offset[0]);
    Ok(result)
}

/// Converts a memory_map JSON object into a vector of SymbolicateMemoryMap
///
/// * `memory_map` - a memory_map as JsonValue with format as follows.
///
/// Example of a memory_map object:  
/// { "memoryMap": [
///     [
///       "xul.pdb",
///       "44E4EC8C2F41492B9369D6B9A059577C2"
///     ],
///     [
///       "wntdll.pdb",
///       "D74F79EB1F8D4A45ABCD2F476CCABACC2"
///     ]
///   ]
/// }
/// When converted into a Vec<SymbolicateMemoryMap>, the vector will look like:
/// vec![ SymbolicateMemoryMap {
///     symbol_file_name: "xul.pdb",
///     debug_id: "44E4EC8C2F41492B9369D6B9A059577C2"
///     }, SymbolicateMemoryMap{
///     symbol_file_name: "wntdll.pdb",
///     debug_id: "D74F79EB1F8D4A45ABCD2F476CCABACC2"
///     } ]
///
pub fn parse_request_memory_map(
    memory_map: &JsonValue,
) -> SymbolicateResult<Vec<SymbolicateMemoryMap>> {
    let mut result = Vec::new();
    for (_, inner_map) in memory_map
        .as_array()
        .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
        .iter()
        .enumerate()
    {
        let memory_map_arr = inner_map
            .as_array()
            .ok_or_else(|| SymbolicateError::JsonParseArrayError)?;
        if memory_map_arr.len() != 2 {
            return Err(SymbolicateError::InvalidInputError(format!(
                "Memory map vector has invalid length. Length must be 2."
            )));
        }
        let symbolicate_memory_map = SymbolicateMemoryMap {
            symbol_file_name: memory_map_arr[0].to_string().replace("\"", ""),
            debug_id: memory_map_arr[1].to_string().replace("\"", ""),
        };
        result.push(symbolicate_memory_map);
    }
    Ok(result)
}

/// Returns a list of candidate path pairs (binary data and debug data)
/// for the given module_name.
/// * `all_candidate_paths_de` - deserialized candidatePaths
/// * `module_name` - name of the module used to retrieve path
pub fn get_candidate_paths_for_module(
    all_candidate_paths_de: &JsonValue,
    module_name: &str,
) -> SymbolicateResult<Vec<CandidatePath>> {
    let paths_for_this_module = all_candidate_paths_de
        .get(module_name)
        .ok_or_else(|| SymbolicateError::NotFoundCandidatePath(module_name.to_string()))?;
    // need to serialize and deserialize to cast the json object into a vector of CandidatePath
    let serialized = serde_json::to_string(paths_for_this_module)
        .or_else(|_| Err(SymbolicateError::JsonParseArrayError))?;
    let module_candidate_paths: Vec<CandidatePath> = serde_json::from_str(&serialized)?;
    return Ok(module_candidate_paths);
}

/// Return the first and the last index of stacks in the SymbolicateRequestStack
/// parititoned by module index
/// Ex. stacks := [[ [63, 2333], [63, 321], [79, 199992] ]]
/// will return [[0, 1], [2, 2]]
pub fn partition_by_module_index(
    symbolicate_job: &SymbolicateJob,
) -> SymbolicateResult<Vec<Vec<usize>>> {
    let mut result = vec![];
    // stacks are preprocessed to be sorted by module_index
    let mut first_index = 0;

    if symbolicate_job.stacks.len() == 1 {
        return Ok(vec![vec![0, 0]]);
    }

    for index in 1..symbolicate_job.stacks.len() {
        if symbolicate_job.get_module_index(index)?
            != symbolicate_job.get_module_index(index - 1)?
        {
            result.push(vec![first_index, index - 1]);
            first_index = index;
        }
        // last element is a unique element by itself, so we push into the array
        if first_index == symbolicate_job.stacks.len() - 1 {
            result.push(vec![first_index, first_index]);
        }
    }
    Ok(result)
}
