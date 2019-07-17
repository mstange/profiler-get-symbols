extern crate profiler_get_symbols;
use profiler_get_symbols::v6::symbolicate_linkage_resolver::*;
use std::fs::File;
use std::io::Read;

/**
 * A little space for testing reading local files
 *  - need to provide: addresses, breakpadId, and file path
 *  - TODO: parse the input from command line instead of hardcoding
 * */

pub fn main() {
    let mut data = Vec::new();
    File::open("/System/Library/Frameworks/OpenGL.framework/Versions/A/OpenGL")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    let result = resolve_to_debug_info_origins(
        &data,
        &[0xd74f],
        &format!("34FA5E8C0FAF3708836BE8ACB67EF4F40"),
    )
    .unwrap();
    println!("result: {:#?}", result);
}
