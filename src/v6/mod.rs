use super::*;
mod response;
pub mod symbolicate_linkage_resolver;
pub mod symbolicate_response_assembler;
pub use crate::symbolicate_common::error::{
    Result as SymbolicateResult, SymbolicateError, SymbolicateErrorJson,
};
pub use crate::symbolicate_common::request::*;
use crate::v6::symbolicate_linkage_resolver::*;

/// Wrapper code to transform the Future struct into wasm-ready JSValue
pub async fn get_all_stack_frames_impl(
    file_path: String,
    read_buffer_callback: js_sys::Function,
    list_of_addresses: Vec<u32>,
    breakpad_id: String,
) -> std::result::Result<JsValue, JsValue> {
    let list_of_addresses_64 = list_of_addresses.iter().map(|x| *x as u64).collect();
    match get_all_stack_frames(
        &file_path,
        &read_buffer_callback,
        &list_of_addresses_64,
        breakpad_id,
    )
    .await
    {
        SymbolicateResult::Ok(resp) => Ok(JsValue::from_serde(&resp).unwrap()),
        SymbolicateResult::Err(err) => {
            let error_type = SymbolicateErrorJson::from_error(err);
            Err(JsValue::from_serde(&error_type).unwrap())
        }
    }
}

/// Return symbolicate json response as a promise to Javascript caller
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
    future_to_promise(
        crate::v6::symbolicate_response_assembler::get_symbolicate_response_impl(
            symbolicate_request,
            read_buffer_callback,
            candidate_paths,
        ),
    )
}
