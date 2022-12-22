# profiler-get-symbols

This repo contains a wasm wrapper around the [`samply-symbols`](https://docs.rs/samply-symbols/) and [`samply-api`](https://docs.rs/samply-api/) crates.

This is used in Firefox to supply symbol information from local files to the [Firefox profiler](https://profiler.firefox.com/):

 - When you capture a profile in Firefox, this profile contains native stacks with code addresses.
 - In order to resolve those code addresses to function names and file/line information, profiler.firefox.com makes a request back into privileged browser code.
 - The privileged browser code downloads the `profiler-get-symbols` wasm file on demand.
 - It executes the wasm code, supplying a "helper" callback object which allows the wasm code to read local files.

The ability to download this code on-demand is the main point of compiling it to WebAssembly. The alternative would have been to compile it into Firefox, but this would have increased the Firefox binary size for everyone.

### WebAssembly

There's a UI for the WebAssembly / JavaScript version in `index.html`.
You can use the files in the `fixtures` directory as examples.

To test, as a one-time setup, install `simple-http-server` using cargo:

```bash
cargo install simple-http-server
```

(The advantage of this over python's `SimpleHTTPServer` is that `simple-http-server` sends the correct mime type for .wasm files.)

Then start the server in this directory, by typing `simple-http-server` and pressing enter:

```bash
$ simple-http-server
     Index: disabled, Upload: disabled, Cache: enabled, Cors: disabled, Range: enabled, Sort: enabled, Threads: 3
          Auth: disabled, Compression: disabled
         https: disabled, Cert: , Cert-Password: 
          Root: /Users/mstange/code/profiler-get-symbols,
    TryFile404: 
       Address: http://0.0.0.0:8000
    ======== [2021-05-28 14:50:47] ========
```

Now you can open [http://127.0.0.1:8000/index.html](http://127.0.0.1:8000/index.html) in your browser and play with the API.

#### Updating the WebAssembly build

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

After a change:

```bash
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/profiler_get_symbols_wasm.wasm --out-dir . --no-modules --no-typescript
```

If this complains about wasm-bindgen version mismatches, update both your local wasm-bindgen-cli and the wasm-bindgen dependency at wasm/Cargo.toml to the latest version.

#### Importing a new version into Firefox

Firefox uses profiler-get-symbols from its [symbolication-worker.js](https://searchfox.org/mozilla-central/rev/553bf8428885dbd1eab9b63f71ef647f799372c2/devtools/client/performance-new/symbolication-worker.js). The wasm file is hosted in a Google Cloud Platform storage bucket (https://storage.googleapis.com/firefox-profiler-get-symbols/). The wasm-bindgen bindings file is checked into the mozilla-central repo, along with the wasm URL and its SRI hash.

Importing a new version of profiler-get-symbols into Firefox is done as follows:

 1. Re-generate the wasm file and the bindings file as described in the previous section.
 2. Commit the new generated files into this git repository, and push to github.
 3. Generate the SRI hash for `profiler_get_symbols_wasm_bg.wasm`: `shasum -b -a 384 profiler_get_symbols_wasm_bg.wasm | awk '{ print $1 }' | xxd -r -p | base64`
 4. Copy the file `profiler_get_symbols_wasm_bg.wasm` to a file named `<current git commit hash>.wasm`.
 5. Upload this file to the GCP bucket.
 6. In mozilla-central, update the following pieces:
   - In `symbolication.jsm.js`, update the .wasm URL, the SRI hash, and the git commit hash.
   - Copy the contents of `profiler_get_symbols_wasm.js` (in this repo) into `profiler_get_symbols.js` (in mozilla-central), leaving the header at the top intact, but updating the commit hash in that header comment.
 7. Address any API changes, if there were any since the last import.
 8. Create a mozilla-central patch with these changes.
