//! Optional standalone launcher; the desktop embeds the same server.
fn main() {
    if !circuitfabric_codex_runtime::judge::run_from_args() {
        eprintln!("Usage: circuitfabric-judge --jev-mcp '<non-secret configuration JSON>'");
        std::process::exit(1);
    }
}
