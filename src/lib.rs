mod error;

use js_sys::Promise;
use serde::Serialize;
use std::{future::Future, pin::Pin};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, JsFuture};

use samply_api::samply_symbols::{
    self, debugid::DebugId, CompactSymbolTable, FileByteSource, FileContentsWithChunkedCaching,
    FileLocation, LibraryInfo,
};

pub use error::{GenericError, GetSymbolsError, JsValueError};

#[wasm_bindgen]
extern "C" {
    pub type FileAndPathHelper;

    /// Returns Array<String>
    /// The strings in the array can be either
    ///   - The path to a binary, or
    ///   - a special string with the syntax "dyldcache:<dyld_cache_path>:<dylib_path>"
    ///     for libraries that are in the dyld shared cache.
    #[wasm_bindgen(catch, method)]
    fn getCandidatePathsForDebugFile(
        this: &FileAndPathHelper,
        library_info: JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, method)]
    fn getCandidatePathsForBinary(
        this: &FileAndPathHelper,
        library_info: JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, method)]
    fn getDyldSharedCachePaths(this: &FileAndPathHelper) -> Result<JsValue, JsValue>;

    /// Returns Promise<BufferWrapper>
    #[wasm_bindgen(method)]
    fn readFile(this: &FileAndPathHelper, path: &str) -> Promise;

    pub type FileContents;

    #[wasm_bindgen(catch, method, getter)]
    fn size(this: &FileContents) -> Result<f64, JsValue>;

    #[wasm_bindgen(catch, method)]
    fn readBytesInto(
        this: &FileContents,
        buffer: js_sys::Uint8Array,
        offset: f64,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn close(this: &FileContents) -> Result<(), JsValue>;
}

/// Usage:
///
/// ```js
/// async function getSymbolTable(debugName, breakpadId, libKeyToPathMap) {
///   const helper = {
///     getCandidatePathsForDebugFile: (info) => {
///       const path = libKeyToPathMap.get(`${info.debugName}/${info.breakpadId}`);
///       if (path !== undefined) {
///         return [path];
///       }
///       return [];
///     },
///     getCandidatePathsForBinary: (info) => [],
///     readFile: async (filename) => {
///       const byteLength = await getFileSizeInBytes(filename);
///       const fileHandle = getFileHandle(filename);
///       return {
///         size: byteLength,
///         readBytesInto: (array, offset) => {
///           syncReadFilePartIntoBuffer(fileHandle, array, offset);
///         },
///         close: () => {},
///       };
///     },
///   };
///
///   const [addr, index, buffer] = await getCompactSymbolTable(debugName, breakpadId, helper);
///   return [addr, index, buffer];
/// }
/// ```
#[wasm_bindgen(js_name = getCompactSymbolTable)]
pub fn get_compact_symbol_table(
    debug_name: String,
    breakpad_id: String,
    helper: FileAndPathHelper,
) -> Promise {
    // console_error_panic_hook::set_once();
    future_to_promise(get_compact_symbol_table_impl(
        debug_name,
        breakpad_id,
        helper,
    ))
}

/// Usage:
///
/// ```js
/// async function queryAPIWrapper(url, requestJSONString, libKeyToPathMap) {
///   const helper = {
///     getCandidatePathsForDebugFile: (info) => {
///       const path = libKeyToPathMap.get(`${info.debugName}/${info.breakpadId}`);
///       if (path !== undefined) {
///         return [path];
///       }
///       return [];
///     },
///     getCandidatePathsForBinary: (info) => [],
///     readFile: async (filename) => {
///       const byteLength = await getFileSizeInBytes(filename);
///       const fileHandle = getFileHandle(filename);
///       return {
///         size: byteLength,
///         readBytesInto: (array, offset) => {
///           syncReadFilePartIntoBuffer(fileHandle, array, offset);
///         },
///         close: () => {},
///       };
///     },
///   };
///
///   const responseJSONString = await queryAPI(url, requestJSONString, helper);
///   return responseJSONString;
/// }
/// ```
#[wasm_bindgen(js_name = queryAPI)]
pub fn query_api(url: String, request_json: String, helper: FileAndPathHelper) -> Promise {
    // console_error_panic_hook::set_once();
    future_to_promise(query_api_impl(url, request_json, helper))
}

async fn query_api_impl(
    url: String,
    request_json: String,
    helper: FileAndPathHelper,
) -> Result<JsValue, JsValue> {
    let symbol_manager = samply_symbols::SymbolManager::with_helper(&helper);
    let api = samply_api::Api::new(&symbol_manager);
    let response_json = api.query_api(&url, &request_json).await;
    Ok(response_json.into())
}

async fn get_compact_symbol_table_impl(
    debug_name: String,
    breakpad_id: String,
    helper: FileAndPathHelper,
) -> Result<JsValue, JsValue> {
    let debug_id = DebugId::from_breakpad(&breakpad_id).map_err(|_| {
        GetSymbolsError::from(samply_symbols::Error::InvalidBreakpadId(breakpad_id))
    })?;
    let symbol_manager = samply_symbols::SymbolManager::with_helper(&helper);
    let info = LibraryInfo {
        debug_name: Some(debug_name),
        debug_id: Some(debug_id),
        ..Default::default()
    };
    let result = symbol_manager.load_symbol_map(&info).await;
    match result {
        Result::Ok(symbol_map) => {
            let table = CompactSymbolTable::from_symbol_map(&symbol_map);
            Ok(js_sys::Array::of3(
                &js_sys::Uint32Array::from(&table.addr[..]),
                &js_sys::Uint32Array::from(&table.index[..]),
                &js_sys::Uint8Array::from(&table.buffer[..]),
            )
            .into())
        }
        Result::Err(err) => Err(GetSymbolsError::from(err).into()),
    }
}

impl FileContents {
    /// Reads `len` bytes at the offset into the memory at dest_ptr.
    /// Safety: The dest_ptr must point to at least `len` bytes of valid memory, and
    /// exclusive access is granted to this function. The memory may be uninitialized.
    /// Safety: This function guarantees that the `len` bytes at `dest_ptr` will be
    /// fully initialized after the call.
    /// Safety: dest_ptr is not stored and the memory is not accessed after this function
    /// returns.
    /// This function does not accept a rust slice because you have to guarantee that
    /// slice contents are fully initialized before you create a slice, and we want to
    /// allow calling this function with uninitialized memory. It is the point of this
    /// function to do the initialization.
    unsafe fn read_bytes_into(
        &self,
        offset: u64,
        len: usize,
        dest_ptr: *mut u8,
    ) -> Result<(), JsValueError> {
        let array = js_sys::Uint8Array::view_mut_raw(dest_ptr, len);
        // Safety requirements:
        // - readBytesInto must initialize all values in the buffer.
        // - readBytesInto must not call into wasm code which might cause the heap to grow,
        //   because that would invalidate the TypedArray's internal buffer
        // - readBytesInto must not hold on to the array after it has returned
        self.readBytesInto(array, offset as f64)
            .map_err(JsValueError::from)
    }
}

pub struct FileContentsWrapper(FileContents);

impl FileByteSource for FileContentsWrapper {
    fn read_bytes_into(
        &self,
        buffer: &mut Vec<u8>,
        offset: u64,
        size: usize,
    ) -> samply_symbols::FileAndPathHelperResult<()> {
        // Make a buffer, wrap a Uint8Array around its bits, and call into JS to fill it.
        // This is implemented in such a way that it avoids zero-initialization and extra
        // copies of the contents.
        buffer.reserve_exact(size);
        unsafe {
            // Safety: The buffer has `size` bytes of capacity.
            // Safety: Nothing else has a reference to the buffer at the moment; we have exclusive access of its contents.
            self.0
                .read_bytes_into(offset, size, buffer.as_mut_ptr().add(buffer.len()))?;
            // Safety: All values in the buffer are now initialized.
            buffer.set_len(buffer.len() + size);
        }
        Ok(())
    }
}

impl Drop for FileContentsWrapper {
    fn drop(&mut self) {
        let _ = self.0.close();
    }
}

impl<'h> samply_symbols::FileAndPathHelper<'h> for FileAndPathHelper {
    type F = FileContentsWithChunkedCaching<FileContentsWrapper>;
    type FL = StringPath;
    type OpenFileFuture =
        Pin<Box<dyn Future<Output = samply_symbols::FileAndPathHelperResult<Self::F>> + 'h>>;

    fn get_candidate_paths_for_debug_file(
        &self,
        library_info: &LibraryInfo,
    ) -> samply_symbols::FileAndPathHelperResult<Vec<samply_symbols::CandidatePathInfo<StringPath>>>
    {
        get_candidate_paths_for_debug_file_impl(
            FileAndPathHelper::from((*self).clone()),
            library_info.clone(),
        )
    }

    fn get_candidate_paths_for_binary(
        &self,
        library_info: &LibraryInfo,
    ) -> samply_symbols::FileAndPathHelperResult<Vec<samply_symbols::CandidatePathInfo<StringPath>>>
    {
        get_candidate_paths_for_binary_impl(
            FileAndPathHelper::from((*self).clone()),
            library_info.clone(),
        )
    }

    fn get_dyld_shared_cache_paths(
        &self,
        _arch: Option<&str>,
    ) -> samply_symbols::FileAndPathHelperResult<Vec<StringPath>> {
        Ok(Vec::new())
    }

    fn get_candidate_paths_for_gnu_debug_link_dest(
        &self,
        debug_link_name: &str,
    ) -> samply_symbols::FileAndPathHelperResult<Vec<StringPath>> {
        // https://www-zeuthen.desy.de/unix/unixguide/infohtml/gdb/Separate-Debug-Files.html
        Ok(vec![
            StringPath(format!("/usr/bin/{}.debug", &debug_link_name)),
            StringPath(format!("/usr/bin/.debug/{}.debug", &debug_link_name)),
            StringPath(format!("/usr/lib/debug/usr/bin/{}.debug", &debug_link_name)),
        ])
    }

    fn get_candidate_paths_for_supplementary_debug_file(
        &self,
        original_file_path: &StringPath,
        sup_file_path: &str,
        sup_file_build_id: &samply_symbols::ElfBuildId,
    ) -> samply_symbols::FileAndPathHelperResult<Vec<StringPath>> {
        let mut paths = Vec::new();

        if sup_file_path.starts_with('/') {
            paths.push(StringPath(sup_file_path.to_owned()));
        } else if let Some(last_slash_pos) = original_file_path.0.rfind(['/', '\\']) {
            let parent_dir = &original_file_path.0[..last_slash_pos];
            paths.push(StringPath(format!("{parent_dir}/{sup_file_path}")));
        }

        let build_id = sup_file_build_id.to_string();
        if build_id.len() > 2 {
            let (two_chars, rest) = build_id.split_at(2);
            let path = format!("/usr/lib/debug/.build-id/{}/{}.debug", two_chars, rest);
            paths.push(StringPath(path));
        }

        Ok(paths)
    }

    fn load_file(
        &self,
        location: StringPath,
    ) -> Pin<Box<dyn Future<Output = samply_symbols::FileAndPathHelperResult<Self::F>> + 'h>> {
        let helper = FileAndPathHelper::from((*self).clone());
        let future = async move {
            let location = location.0;
            let file_res = JsFuture::from(helper.readFile(&location)).await;
            let file = file_res.map_err(JsValueError::from)?;
            let contents = FileContents::from(file);
            let len = contents.size().map_err(JsValueError::from)? as u64;
            let file_contents_wrapper = FileContentsWrapper(contents);
            Ok(FileContentsWithChunkedCaching::new(
                len,
                file_contents_wrapper,
            ))
        };
        Box::pin(future)
    }
}

#[derive(Debug, Clone)]
pub struct StringPath(String);

impl std::fmt::Display for StringPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FileLocation for StringPath {
    fn location_for_dyld_subcache(&self, suffix: &str) -> Option<Self> {
        Some(Self(format!("{}{suffix}", self.0)))
    }

    fn location_for_external_object_file(&self, object_file: &str) -> Option<Self> {
        Some(Self(object_file.to_owned()))
    }

    fn location_for_pdb_from_binary(&self, pdb_path_in_binary: &str) -> Option<Self> {
        Some(Self(pdb_path_in_binary.to_owned()))
    }

    fn location_for_source_file(&self, source_file_path: &str) -> Option<Self> {
        Some(Self(source_file_path.to_owned()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsLibraryInfo {
    pub debug_name: Option<String>,
    pub breakpad_id: Option<String>,
    pub debug_path: Option<String>,
    pub name: Option<String>,
    pub code_id: Option<String>,
    pub path: Option<String>,
    pub arch: Option<String>,
}

impl From<LibraryInfo> for JsLibraryInfo {
    fn from(info: LibraryInfo) -> Self {
        Self {
            debug_name: info.debug_name,
            breakpad_id: info.debug_id.map(|di| di.breakpad().to_string()),
            debug_path: info.debug_path,
            name: info.name,
            code_id: info.code_id.map(|ci| ci.to_string()),
            path: info.path,
            arch: info.arch,
        }
    }
}

fn make_library_info_js_value(library_info: LibraryInfo) -> JsValue {
    serde_wasm_bindgen::to_value(&JsLibraryInfo::from(library_info)).unwrap()
}

fn get_candidate_paths_for_debug_file_impl(
    helper: FileAndPathHelper,
    library_info: LibraryInfo,
) -> samply_symbols::FileAndPathHelperResult<Vec<samply_symbols::CandidatePathInfo<StringPath>>> {
    let paths = helper
        .getCandidatePathsForDebugFile(make_library_info_js_value(library_info))
        .map_err(JsValueError::from)?;
    let array = js_sys::Array::from(&paths);
    Ok(convert_js_array_to_candidate_paths(array))
}

fn get_candidate_paths_for_binary_impl(
    helper: FileAndPathHelper,
    library_info: LibraryInfo,
) -> samply_symbols::FileAndPathHelperResult<Vec<samply_symbols::CandidatePathInfo<StringPath>>> {
    let paths = helper
        .getCandidatePathsForBinary(make_library_info_js_value(library_info))
        .map_err(JsValueError::from)?;
    let array = js_sys::Array::from(&paths);
    Ok(convert_js_array_to_candidate_paths(array))
}

fn convert_js_array_to_candidate_paths(
    array: js_sys::Array,
) -> Vec<samply_symbols::CandidatePathInfo<StringPath>> {
    array
        .iter()
        .filter_map(|val| val.as_string())
        .map(|s| {
            // Support special syntax "dyldcache:<dyld_cache_path>:<dylib_path>"
            if let Some(remainder) = s.strip_prefix("dyldcache:") {
                if let Some(offset) = remainder.find(':') {
                    let dyld_cache_path = &remainder[0..offset];
                    let dylib_path = &remainder[offset + 1..];
                    return samply_symbols::CandidatePathInfo::InDyldCache {
                        dyld_cache_path: StringPath(dyld_cache_path.into()),
                        dylib_path: dylib_path.into(),
                    };
                }
            }
            samply_symbols::CandidatePathInfo::SingleFile(StringPath(s))
        })
        .collect()
}
