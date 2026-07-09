#![allow(dead_code, unused_imports, unused_variables)]

mod cli {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/cli.rs"));
}
mod commands {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/commands.rs"));
}
mod completion {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/completion.rs"));
}
mod editor {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/editor.rs"));
}
mod frontend {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/frontend.rs"));
}
mod highlighter {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/highlighter.rs"));
}
mod kernel {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/kernel.rs"));
}
mod native_wstp {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/native_wstp.rs"));
}
mod profiler {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/profiler.rs"));
}
mod repl {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/repl.rs"));
}
mod theme {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/theme.rs"));
}
mod wl {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/wl.rs"));
}
mod wolfram_syntax {
    include!(concat!(env!("WOLFRAM_CLI_ROOT"), "/src/wolfram_syntax.rs"));
}

use cli::*;
use commands::*;
use completion::*;
use editor::*;
use frontend::*;
use highlighter::*;
use kernel::*;
use native_wstp::*;
use profiler::*;
use repl::*;
use theme::*;
use wl::*;
use wolfram_syntax::*;

println!("Loaded wolfram-cli modules. Try: parse_repl_command(\":help\")");
