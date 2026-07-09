# Wolfram CLI
## Usage

Start the interactive REPL. This uses the native WSTP backend and keeps a kernel session alive for REPL state:

```sh
cargo run
```

Evaluate one expression and exit:

```sh
cargo run -- -e 'Range[5]^2'
```

Run a script file through `wolframscript`:

```sh
cargo run -- path/to/script.wls -- arg1 arg2
```



## Local Rust REPL with evcxr

Start a project-configured Rust REPL for trying crate-visible helpers and Wolfram calls directly:

```sh
./evcxr-local.sh
```

The launcher builds the project, points `evcxr` at `.evcxr/evcxr/init.evcxr`, and loads the modules in `src/` into the REPL. After startup you can call crate-visible items directly, for example:

```rust
parse_repl_command(":help")
wolfram_string_literal("Range[5]")
let mut kernel = KernelClient::new()?;
kernel.query_string("Range[5]^2")?
```

Use `:quit` to leave the REPL.

## Completion

The REPL opens an IDE-style completion popup dynamically as you type symbol characters. Use `Tab` to cycle/accept entries, `Shift+Tab` to move backward, and `Esc` to close the popup.

Symbol completions are queried from the active kernel session as you type, so user-defined symbols, functions, and loaded package symbols are included after each evaluation. The query uses prefix-shaped `Names` calls, for example:

```wl
Names[prefix <> "*"]
```

Matching context names are suggested from `Contexts[]`, and qualified input such as `MyContext`My` queries symbols inside that context.

When the cursor is inside a function call after the first top-level comma, option completions are loaded lazily from:

```wl
Options[head]
```

For example, `Plot[x, {x, 0, 1}, PlotR` can complete `PlotRange`.

By default the REPL also initializes Wolfram FrontEnd services in the background when they can be discovered. This is used as the boundary for future FrontEnd-backed functionality such as graphics rendering without opening a notebook window. If the FrontEnd cannot be initialized, the REPL continues with the kernel-only engine.

Disable FrontEnd integration and use the simpler kernel-only completion engine with:

```sh
cargo run -- --no-frontend
```

## REPL Commands

Lines that start with `:` are handled by the CLI instead of being evaluated as Wolfram Language input:

```text
:clear
:help
:theme
:theme dark|light|solarized|gruvbox|monokai|plain
:theme list
:theme show
:quit
```

`:clear` clears the console. `:theme` cycles the syntax highlighting theme. `:theme list` previews available themes. `:quit` exits the REPL; `Exit`, `Quit`, and Ctrl-D are also supported.

Command completions are available only when the line starts with `:`. Wolfram Language completions are disabled for those command lines.

## Kernel Discovery

Set `WOLFRAM_KERNEL` to override the kernel executable. Without that override, the CLI asks `wolframscript -showkernels` for the best local kernel path, falls back to `wolfram-app-discovery`, and prefers the native kernel binary under `SystemFiles/Kernel/Binaries` before falling back to `WolframKernel` on `PATH`.

Set `WOLFRAM_FRONTEND` to override the FrontEnd executable used for FrontEnd-backed completions and rendering support.

The `wstp` crate links Wolfram's WSTP static library at build time. A build machine must have a Wolfram installation or WSTP SDK for the Rust target being built. If discovery does not find it, set `WSTP_COMPILER_ADDITIONS_DIRECTORY` to the target's `SystemFiles/Links/WSTP/DeveloperKit/<SystemID>/CompilerAdditions` directory.

Runtime expression evaluation requires WSTP; there is no subprocess fallback.

## Release Builds

GitHub Actions builds packaged binaries when a `v*` or `build*` tag is pushed. `test*` tags and manual workflow runs exercise the build/test path without packaging or publishing artifacts, unless the manual run is explicitly started from a `v*` or `build*` tag ref.

Release builds run on GitHub-hosted runners. Because GitHub-hosted runners do not include Wolfram and `wstp-sys` links the target WSTP static library during `cargo build`, the workflow extracts the required `CompilerAdditions` from official Wolfram Engine artifacts before building:

| Artifact | Runner | Rust target | WSTP source |
| --- | --- | --- | --- |
| `linux-x86_64` | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | Wolfram Engine Docker image |
| `macos-x86_64` | `macos-15-intel` | `x86_64-apple-darwin` | Wolfram Engine macOS DMG |
| `macos-aarch64` | `macos-15` | `aarch64-apple-darwin` | Wolfram Engine macOS DMG |
| `windows-x86_64` | `windows-latest` | `x86_64-pc-windows-msvc` | Wolfram Engine Windows MSI |

Locally, set `WSTP_COMPILER_ADDITIONS_DIRECTORY` if automatic discovery does not find the target's `SystemFiles/Links/WSTP/DeveloperKit/<SystemID>/CompilerAdditions` directory. Linux builds also need the system `uuid` library available for linking, for example the `uuid-dev` package on Debian/Ubuntu systems.

The packaged binary locates the user's Wolfram installation at runtime using the discovery behavior above. Expression, REPL, and completion evaluation run over WSTP; script files are delegated to `wolframscript`.
