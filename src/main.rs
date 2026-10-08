//! Main executable entry point for Swagger / OpenAPI plugin.

use std::env;
use std::process;

use mpe_plugin_openapi::rpc::{run_self_test, run_stdio_loop};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|arg| arg == "--self-test") {
        match run_self_test().await {
            Ok(()) => process::exit(0),
            Err(e) => {
                eprintln!("[Self-Test Error] {e}");
                process::exit(1);
            }
        }
    }

    run_stdio_loop().await;
}
