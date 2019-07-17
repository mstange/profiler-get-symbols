extern crate serde;
extern crate serde_derive;
extern crate wasm_bindgen;
use super::*;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Serialize, Clone)]
pub struct SymbolicateResponseResult {
    pub stacks: Vec<Vec<SymbolicateResponseStack>>,
    pub found_modules: SymbolicateFoundModule,
    pub errors: HashMap<String, String>,
}

#[derive(Default, Serialize, Clone)]
pub struct SymbolicateResponseStack {
    pub module_offset: String,
    pub module: String,
    pub frame: usize,
    pub function: Option<String>,
    pub function_offset: Option<String>,
}

pub struct ResponseStackResult {
    pub stacks: Vec<SymbolicateResponseStack>,
    pub is_module_found: bool,
}

#[derive(Serialize, Clone)]
pub struct SymbolicateFoundModule {
    symbolicate_found_status: HashMap<String, bool>,
}

#[derive(Serialize)]
pub struct SymbolicateResponseJson {
    pub results: Vec<SymbolicateResponseResult>,
}

impl SymbolicateResponseJson {
    pub fn as_json(&self) -> JsonValue {
        json!({
            "results": self.results.iter().map(|result| result.as_json()).collect::<Vec<_>>()
        })
    }

    pub fn new() -> Self {
        SymbolicateResponseJson {
            results: Vec::new(),
        }
    }

    pub fn push(&mut self, symbolicate_response_result: SymbolicateResponseResult) {
        self.results.push(symbolicate_response_result);
    }
}

impl SymbolicateResponseStack {
    pub fn as_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap()
    }

    pub fn from(&mut self, function_info: SymbolicateFunctionInfo) {
        self.function = function_info.function;
        self.function_offset = function_info.function_offset;
    }
}

impl SymbolicateResponseResult {
    pub fn as_json(&self) -> JsonValue {
        json!({
            "stacks" : self.stacks.iter().map(|vec| vec.iter().map(|stack| stack.as_json()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "found_modules": self.found_modules.as_json(),
            "errors": serde_json::to_value(&self.errors).unwrap(),
        })
    }

    pub fn new() -> Self {
        SymbolicateResponseResult {
            stacks: vec![],
            found_modules: SymbolicateFoundModule::new(),
            errors: HashMap::new(),
        }
    }

    pub fn push(&mut self, stacks: Vec<SymbolicateResponseStack>) {
        self.stacks.push(stacks);
    }
}

impl SymbolicateFoundModule {
    pub fn new() -> Self {
        SymbolicateFoundModule {
            symbolicate_found_status: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, val: bool) {
        self.symbolicate_found_status.insert(key, val);
    }

    pub fn as_json(&self) -> JsonValue {
        serde_json::to_value(&self.symbolicate_found_status).unwrap()
    }
}
