use super::*;
mod response;
mod symbolicate_response_assembler;
use super::CompactSymbolTable;
use crate::symbol_table::compact_symbol_table::*;
pub use crate::symbolicate_common::error::{
    Result as SymbolicateResult, SymbolicateError, SymbolicateErrorJson,
};
pub use crate::symbolicate_common::request::*;
use crate::v5::symbolicate_response_assembler::*;

/// Return symbolicate json response as a promise to Javascript caller in v5 format
///
/// Input:
/// * `symbolicate_request` - Symbolicate request
/// * `read_buffer_callback` - Javascript callback that test and get
///     the buffer from the path if valid
/// * `candidate_paths` - JSON of candidate paths
///
/// Example input format for `candidate_paths`:
/// {
///     "xul.pdb" : [{
///         "path": <...>
///         "debugPath": <...>
///     }]
/// }
pub fn get_symbolicate_response(
    symbolicate_request: JsValue,
    read_buffer_callback: js_sys::Function, // read_buffer_callback takes two param: fileName, debugId
    candidate_paths: JsValue,
) -> Promise {
    future_to_promise(get_symbolicate_response_impl(
        symbolicate_request,
        read_buffer_callback,
        candidate_paths,
    ))
}

#[cfg(test)]
mod test {
    use super::*;
    extern crate serde_json;
    extern crate wasm_bindgen;
    extern crate wasm_bindgen_test;
    use super::symbolicate_common::error::Result as SymbolicateResult;
    use super::symbolicate_common::error::*;
    use super::v5::response::*;
    use serde_json::Value as JsonValue;
    extern crate wasm_bindgen_futures;
    use wasm_bindgen::prelude::*;

    #[test]
    pub fn test_partition_by_module_index() {
        // TODO
    }

    #[test]
    pub fn test_symb_result_insert_stack() {
        let mut result = SymbolicateResponseResult::new();

        let stack = SymbolicateResponseStack {
            module_offset: "0xb2e3f7".to_string(),
            module: "xul.pdb".to_string(),
            frame: 0,
            function: Some("KiUserCallbackDispatcher".to_string()),
            function_offset: None,
        };

        result.push(vec![stack]);
        assert_eq!(result.stacks[0][0].module_offset, "0xb2e3f7");
        assert_eq!(result.stacks[0][0].module, "xul.pdb");
        assert_eq!(result.stacks[0][0].frame, 0);
        assert_eq!(
            result.stacks[0][0].function,
            Some("KiUserCallbackDispatcher".to_string())
        );
    }

    #[test]
    pub fn test_get_candidate_paths_for_module() {
        let candidate_paths = r#"{
            "libxul.so" : [
                {
                    "path": "../org.mozilla.geckoview_example-1/lib/arm/libxul.so",
                    "debugPath": "../org.mozilla.geckoview_example-1/lib/arm/libxul.so"
                }, 
                {
                    "path": "../dist/bin/libxul.so",
                    "debugPath": "../dist/bin/libxul.so"
                }
            ],
            "xil.pdb" : [
                {
                    "path": "../dist/bin/xil.pdb",
                    "debugPath": "../dist/bin/xil.pdb"
                }
            ]
        }"#;
        let all_candidate_paths_de: JsonValue = serde_json::from_str(candidate_paths).unwrap();
        assert_eq!(
            all_candidate_paths_de
                .get("libxul.so")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let candidate_paths_for_libxul: Vec<CandidatePath> =
            get_candidate_paths_for_module(&all_candidate_paths_de, &String::from("libxul.so"))
                .unwrap();
        assert_eq!(candidate_paths_for_libxul.len(), 2);
        assert_eq!(
            candidate_paths_for_libxul[0].path,
            candidate_paths_for_libxul[0].debugPath
        );
        assert_eq!(
            candidate_paths_for_libxul[0].debugPath,
            String::from("../org.mozilla.geckoview_example-1/lib/arm/libxul.so")
        );

        let candidate_paths_for_xil: Vec<CandidatePath> =
            get_candidate_paths_for_module(&all_candidate_paths_de, &String::from("xil.pdb"))
                .unwrap();
        assert_eq!(candidate_paths_for_xil.len(), 1);
        assert_eq!(
            candidate_paths_for_xil[0].path,
            String::from("../dist/bin/xil.pdb")
        );

        // Negative case: doesn't exist
        assert!(get_candidate_paths_for_module(
            &all_candidate_paths_de,
            &String::from("zoozooo.pdb")
        )
        .is_err());
    }

    fn get_correct_job_1() -> &'static str {
        r#"{
            "jobs": [
            {
                "memoryMap": [
                    [
                        "xul.pdb",
                        "0C920D17E0193CFA8B026CC14D90C0DB0"
                    ]
                ],
                "stacks": [
                    [
                        [0, 9604]  
                    ]
                ]
            }
            ]
        }"#
    }

    fn get_correct_job_2() -> &'static str {
        r#"{
                "memoryMap": [
                    [
                        "xul.pdb",
                        "0C920D17E0193CFA8B026CC14D90C0DB0"
                    ]
                ],
                "stacks": [
                    [
                        [0, 9604]  
                    ]
                ]
        }"#
    }

    #[wasm_bindgen_test]
    pub fn test_parse_symbolicate_request() {
        // let jsvalue_placeholder = JsValue::NULL;
        // let js_sys_placeholder = js_sys::Function::new_no_args("placeholder");
        assert!(parse_symbolicate_request(&JsValue::from("{}")).is_err());
        // To test, must convert to serde_json, and then to JsValue
        // Directly from string to JsValue will cause the tests to fail
        for job_str in &[get_correct_job_1(), get_correct_job_2()] {
            let json: JsonValue = serde_json::from_str(job_str).unwrap();
            let jobs_result = parse_symbolicate_request(&JsValue::from_serde(&json).unwrap());
            assert!(jobs_result.is_ok());
            let jobs = jobs_result.unwrap();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].memory_map.len(), 1);
            assert_eq!(jobs[0].memory_map[0].symbol_file_name, "xul.pdb");
            assert_eq!(
                jobs[0].memory_map[0].debug_id,
                "0C920D17E0193CFA8B026CC14D90C0DB0"
            );
            assert_eq!(jobs[0].stacks.len(), 1);
            assert_eq!(jobs[0].stacks[0].get_module_offset(), 9604);
            assert_eq!(jobs[0].stacks[0].get_module_index(), 0);
        }
    }

    #[wasm_bindgen_test]
    pub fn test_parse_request_stacks() {
        let jobs: JsonValue = serde_json::from_str(
            r#"[
                [
                    [0, 9604] 
                ]
            ]"#,
        )
        .unwrap();
        let stacks_result: SymbolicateResult<Vec<SymbolicateRequestStack>> =
            parse_request_stacks(&jobs);
        assert!(stacks_result.is_ok());
        let stacks = stacks_result.unwrap();
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].get_module_index(), 0);
        assert_eq!(stacks[0].get_module_offset(), 9604);

        // Negative case:
        // Case 1: Length of the array is too short
        let negative_job: JsonValue = serde_json::from_str(
            r#"[
                [
                    [0]
                ]
        ]"#,
        )
        .unwrap();
        assert!(parse_request_stacks(&negative_job).is_err());

        // Case 2: Length of the array is too long
        let negative_job: JsonValue = serde_json::from_str(
            r#"[
                [
                    [0, 100, 500]
                ]
        ]"#,
        )
        .unwrap();
        assert!(parse_request_stacks(&negative_job).is_err());

        // Case 3: empty array
        let negative_job: JsonValue = serde_json::from_str(
            r#"[
            [
            ]
        ]"#,
        )
        .unwrap();
        assert!(parse_request_stacks(&negative_job).is_err());
    }

    #[wasm_bindgen_test]
    pub fn test_parse_request_memory_map() {
        let memory_map: JsonValue = serde_json::from_str(
            r#"[
            [
                "xul.pdb",
                "0C920D17E0193CFA8B026CC14D90C0DB0"
            ],
            [
                "zooozooo.pdb",
                "1234566"
            ]
        ]"#,
        )
        .unwrap();

        let memory_map_result = parse_request_memory_map(&memory_map);
        assert!(memory_map_result.is_ok());
        let memory_map: Vec<SymbolicateMemoryMap> = memory_map_result.unwrap();
        assert_eq!(memory_map.len(), 2);
        assert_eq!(memory_map[0].debug_id, "0C920D17E0193CFA8B026CC14D90C0DB0");
        assert_eq!(memory_map[0].symbol_file_name, "xul.pdb");
        assert_eq!(
            memory_map[0].as_string(),
            "xul.pdb/0C920D17E0193CFA8B026CC14D90C0DB0"
        );
        assert_eq!(memory_map[1].as_string(), "zooozooo.pdb/1234566");
    }

    #[wasm_bindgen_test]
    pub fn test_parse_request_job() {
        let symbolicate_job: JsonValue = serde_json::from_str(&get_correct_job_2()).unwrap();
        let symbolicate_job_res: SymbolicateResult<SymbolicateJob> =
            parse_request_job(&symbolicate_job);
        assert!(symbolicate_job_res.is_ok());
        let symbolicate_job = symbolicate_job_res.unwrap();
        assert_eq!(symbolicate_job.memory_map.len(), 1);
        assert_eq!(
            symbolicate_job.memory_map[0].debug_id,
            "0C920D17E0193CFA8B026CC14D90C0DB0"
        );
        assert_eq!(symbolicate_job.memory_map[0].symbol_file_name, "xul.pdb");
    }

    #[wasm_bindgen_test]
    pub fn test_serde() {
        let data: JsonValue = serde_json::from_str(get_correct_job_1()).unwrap();
        assert!(data.get("jobs").is_some());
    }
}
