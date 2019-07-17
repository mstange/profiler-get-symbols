use crate::symbol_table::error::GetSymbolsError;
use gimli::Error as gimliError;
use goblin::error::Error as goblinError;
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use std::fmt::{self};
use std::num::ParseIntError;
use wasm_bindgen::JsValue;
pub type Result<T> = std::result::Result<T, SymbolicateError>;

#[derive(Debug)]
pub enum SymbolicateError {
    InvalidInputError(String),
    UnmatchedModuleIndex(usize, usize),
    /// Expected,
    ModuleIndexOutOfBound(usize, usize, usize),
    CompactSymbolTableError(GetSymbolsError),
    JsonParseArrayError,
    CallbackError,
    SerdeConversionError(SerdeJsonError),
    JsValueError(JsValue),
    NotFoundCandidatePath(String),
    UnfoundInlineFrames,
}

impl fmt::Display for SymbolicateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SymbolicateError::InvalidInputError(ref invalid_input) => {
                write!(f, "Invalid input: {}", invalid_input)
            }
            SymbolicateError::UnmatchedModuleIndex(ref expected, ref actual) => write!(
                f,
                "Unmatched module index: Expected {}, but received {}",
                expected, actual
            ),
            SymbolicateError::ModuleIndexOutOfBound(min_index, max_index, module_index) => write! (
                f,
                "ModuleIndexOutOfBound: Minimum index is {} and max index is {}, but received {} as module index", min_index, max_index, module_index
            ),
            SymbolicateError::CompactSymbolTableError(ref get_symbol_error) => {
                write!(f, "GetSymbolsError error: {:?}", get_symbol_error.to_string())
            },
            SymbolicateError::JsonParseArrayError => {
                write!(f, "JsonParseArrayError")
            },
            SymbolicateError::SerdeConversionError(ref serde_json_error) => {
                write!(f, "SerdeConversionError: {}", serde_json_error.to_string())
            },
            SymbolicateError::JsValueError(ref js_value) => {
                write!(f, "{:?}",  js_value)
            },
            SymbolicateError::CallbackError => {
                write!(f, "CallbackError: ")
            },
            SymbolicateError::NotFoundCandidatePath(ref module_name) => {
                write!(f, "NotFoundCandidatePath: Cannot find candidate paths for {}", module_name)
            },
            SymbolicateError::UnfoundInlineFrames => {
                write!(f, "UnfoundInlineFrames: The inline frame is not found")
            }
        }
    }
}

impl From<SerdeJsonError> for SymbolicateError {
    fn from(err: SerdeJsonError) -> SymbolicateError {
        SymbolicateError::SerdeConversionError(err)
    }
}

impl From<JsValue> for SymbolicateError {
    fn from(err: JsValue) -> SymbolicateError {
        SymbolicateError::JsValueError(err)
    }
}

impl From<GetSymbolsError> for SymbolicateError {
    fn from(err: GetSymbolsError) -> SymbolicateError {
        SymbolicateError::CompactSymbolTableError(err)
    }
}

impl From<ParseIntError> for SymbolicateError {
    fn from(err: ParseIntError) -> SymbolicateError {
        SymbolicateError::InvalidInputError(err.to_string())
    }
}

impl From<&str> for SymbolicateError {
    fn from(err: &str) -> SymbolicateError {
        SymbolicateError::InvalidInputError(err.to_string())
    }
}

impl From<goblinError> for SymbolicateError {
    fn from(err: goblinError) -> SymbolicateError {
        SymbolicateError::InvalidInputError(err.to_string())
    }
}

impl From<gimliError> for SymbolicateError {
    fn from(err: gimliError) -> SymbolicateError {
        SymbolicateError::InvalidInputError(err.to_string())
    }
}

impl SymbolicateError {
    pub fn enum_as_string(&self) -> &'static str {
        match *self {
            SymbolicateError::InvalidInputError(_) => "InvalidInputError",
            SymbolicateError::UnmatchedModuleIndex(_, _) => "UnmatchedModuleIndex",
            SymbolicateError::ModuleIndexOutOfBound(_, _, _) => "ModuleIndexOutOfBound",
            SymbolicateError::CompactSymbolTableError(_) => "CompactSymbolTableError",
            SymbolicateError::JsonParseArrayError => "JsonParseArrayError",
            SymbolicateError::SerdeConversionError(_) => "SerdeConversionError",
            SymbolicateError::JsValueError(_) => "JsValueError",
            SymbolicateError::CallbackError => "CallbackError",
            SymbolicateError::NotFoundCandidatePath(_) => "NotFoundCandidatePath",
            SymbolicateError::UnfoundInlineFrames => "UnfoundInlineFrames",
        }
    }
}

#[derive(Serialize)]
pub struct SymbolicateErrorJson {
    error_type: String,
    error_msg: String,
}

impl SymbolicateErrorJson {
    pub fn from_error(err: SymbolicateError) -> Self {
        SymbolicateErrorJson {
            error_type: err.enum_as_string().to_string(),
            error_msg: err.to_string(),
        }
    }
}
