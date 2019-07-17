# profiler-get-symbols

This repo contains a WebAssembly wrapper which allows dumping symbol tables from
ELF and Mach-O binaries as well as from pdb files. It is a relatively thin
wrapper around the crates `object`, `goblin` and `pdb`.

The resulting .wasm file is used by the Gecko profiler; more specifically, it is
used by the [ProfilerGetSymbols.jsm](https://searchfox.org/mozilla-central/source/browser/components/extensions/ProfilerGetSymbols.jsm) module in Firefox. The code is run every time you use the Gecko profiler: On macOS and Linux
it is used to get symbols for native system libraries, and on all platforms it
is used if you're profiling a local build of Firefox for which there are no
symbols on the [Mozilla symbol server](https://symbols.mozilla.org/).

## Table of contents
  - [Building](#building)
  - [Running / Testing](#running--testing)
  - [Publishing](#publishing)
  - [Demo](#demo)
    - [Compact symbol table](#compact-symbol-table)
    - [Local symbolication v5](#local-symbolication-v5)
    - [Local symbolication v6](#local-symbolication-v6)
    - [Inline call](#inline-call)
  - [Brief history](#brief-history)
  - 


## Building

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli # --force to update
```

On changes:

```bash
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/profiler_get_symbols.wasm --out-dir . --no-modules --no-typescript
shasum -b -a 384 profiler_get_symbols_bg.wasm | awk '{ print $1 }' | xxd -r -p | base64 # This is your SRI hash, update it in index.html
```

## Running / Testing

This repo contains a minimal `index.html` which lets you test the resulting wasm
module manually in the browser. However, you need a file to test it on; this
repo does not contain a test binary.

To test, as a one-time setup, install http-server using cargo:

```bash
cargo install http-server
```

(The advantage of this over python's `SimpleHTTPServer` is that `http-server` sends the correct mime type for .wasm files.)

Then start the server in this directory, by typing `http-server` and pressing enter:

```bash
$ http-server
Starting up http-server, serving ./
Available on:
  http://0.0.0.0:8080
Hit CTRL-C to stop the server
```

Now:

 1. Open [http://0.0.0.0:8080](http://0.0.0.0:8080) in your browser.
 2. Use the file inputs to select your binary:
    - For Windows binaries, put the exe file in the first field and the pdb file in the second field.
    - For Mach-O and ELF binaries, load that same binary into both fields.
 3. Enter the breakpad ID for the binary into the text field. (I promise to add a better explanation for this step in the future.)
 4. Hit the button.
 5. Open the web console in your browser's devtools. There should be some numbers there.

Unfortunately the symbols are not printed in a very human-readable format currently.
Instead, the symbol table is output in the [`SymbolTableAsTuple` format](https://github.com/firefox-devtools/profiler/blob/40a56a1f305bd8726fa366b72a43287261a254a8/src/profile-logic/symbol-store-db.js#L17-L40),
which is the format that the Firefox profiler front-end expects.

## Publishing

At the moment, the resulting wasm files are hosted in a separate repo called
[`profiler-assets`](https://github.com/mstange/profiler-assets/), in the
[`assets/wasm` directory](https://github.com/mstange/profiler-assets/tree/master/assets/wasm).
The filename of each of those wasm file is the same as its SRI hash value, but expressed in hexadecimal
instead of base64. Here's a command which creates a file with such a name from your `profiler_get_symbols_bg.wasm`:

```bash
cp profiler_get_symbols_bg.wasm `shasum -b -a 384 profiler_get_symbols_bg.wasm | awk '{ print $1 }'`.wasm
```

## Demo
### Compact symbol table
To test retrieving a compact symbol table, type the following in the developer console on your local Nightly build.

```
<IMPORT ProfilerGetSymbols>

ProfilerGetSymbols.getSymbolTable(binaryPath, debugPath, breakpadId);
```
where the `binaryPath`, `debugPath`, and `breakpadId` are all found in the shared library metadata. 

### Local symbolication v5
To test the API v5 endpoint, type the following in the developer console on your local Nightly build. The format of the request JSON is exactly the same as that specified on Tecken v5. To find the path to a library or load ProfilerGetSymbols, please refer to [FAQ](#faq). Specifically, the "path to xul" is exactly the one given in finding the path according to FAQ. 

```
<IMPORT ProfilerGetSymbols>
let candidatePaths = {
  "xul.pdb": [
    {
      "path": <INSERT PATH TO XUL>,
      "debugPath": <INSERT PATH TO XUL>
    },
    {
      "path": "GarbagePath",
      "debugPath": "GarbagePath2"
    }
  ]
};

let symbolicateRequest = {
"jobs": [
    {
      "memoryMap": [
        [
          "xul.pdb",
          "<INSERT XUL BREAKPAD ID>"
        ],
      ],
      "stacks": [
        [
          [0, 20009],
        ]
      ]
    }
  ]
};

let result = await ProfilerGetSymbols.getSymbolicateResponse(candidatePaths, symbolicateRequest, "symbolicate/v5");
```

### Local symbolication v6
To test the API v6 endpoint, type the following in the developer console on your local Nightly build. This endpoint uses the same request formats as v6, it's the output JSON that differs.

```
<IMPORT ProfilerGetSymbols>
<SAME candidatePaths and symbolicateRequest AS ABOVE>
let result = await ProfilerGetSymbols.getSymbolicateResponse(candidatePaths, symbolicateRequest, "symbolicate/v6");
```

### Inline call
To test the inline function, import ProfilerGetSymbols then call `ProfilerGetSymbols.getInlineFrames(path, addresses, breakpad_id)`.

## Brief History
#### Updated: Aug 30, 2019
Since local Firefox nightly builds cannot use the symbolication server (tecken) due to unmatched breakpad IDs, in order to display symbolication for functions on the local builds it is necessary to read the binary files locally. 

Previously, this library would return arrays of bytes, representing the symbol table, for the specific library through the Rust wasm-bindgen function `get_compact_symbol_table`. Since this code is incorporated into Gecko, Gecko would return then symbol table as a promise to the profiler. 

However, this method is rather difficult to maintained, since it requires 3rd-party developers— those who would like to use symbolication information— know the specific implementation details of parsing the symbol table.

Therefore, we decide to set up a local symbolication endpoint (v5) that returns the same format as the Tecken API, but can process binaries locally. This ensures that Gecko will now pass on a JSON response object to the profiler instead of the symbols table that can require specific domain knowledge to parse. 

Now, as an enhancement, for the binary files that also have debug information— such as firefox, XUL, and some other non-system libraries— the new v6 endpoint will return inline information as well.

## FAQ
### 1. How to find the debug path and path of a library?

Enter the following in the developer console of your local Gecko: 
```
libs = Services.profiler.sharedLibraries
```
Then `libs` will contain all the metadata about each shared library.

### 2. How to load the profiler?

```
Cu.import("resource://gre/modules/ProfilerGetSymbols.jsm");
```

### 3. How to find valid addresses of locally defined functions of an executable? 
  You can either `nm` dump the binary (on Mac or Linux), or use `ProfilerGetSymbols.getSymbolTable` as instructed in the demo section.