//! `wf-mcp`: the Buu's Fury agent MCP server (stdio). Headless and muted; content is extracted from the ROM on
//! first use and cached (see `wf_mcp::content`). `wf-mcp --check` loads the content and prints a summary;
//! `wf-mcp --call <tool> '<json args>'` runs one tool call and prints its JSON (for shells and smoke tests).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--check") => match wf_mcp::content::game_data() {
            Ok(d) => println!("content OK: {} enemy types, {} skills, {} forms; cache {}", d.enemies.len(), d.skills.len(), d.forms.len(), wf_mcp::content::content_dir().unwrap().display()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Some("--call") => {
            let mut srv = wf_mcp::tools::Server::new();
            let a: serde_json::Value = args.get(2).map(|s| serde_json::from_str(s).expect("args JSON")).unwrap_or(serde_json::json!({}));
            match srv.call(args.get(1).map(String::as_str).unwrap_or(""), &a) {
                Ok(wf_mcp::tools::Out::Json(v)) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
                Ok(wf_mcp::tools::Out::Image(_, cap)) => println!("{cap}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Some("-h") | Some("--help") => println!("wf-mcp: MCP server on stdio (no arguments). --check | --call <tool> '<json>'. Env: WF_ROM, WF_CONTENT, WF_CACHE."),
        _ => wf_mcp::mcp::serve(),
    }
}
