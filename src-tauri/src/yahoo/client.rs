use std::path::PathBuf;
use serde_json::Value;

/// Python is the sole Yahoo fetch method — source kept at `py-scripts/yahoo_stock_extract.py`
/// (perfect 4yr + TTM via yfinance, mirrored in `input_findata_example*.json`).
/// Runtime is multi-tier: bundled portable python in `resources/python/` for packaged builds,
/// host `python3` + `py-scripts/extractor_cli.py` for `cargo tauri dev` / `cargo test`.

enum PythonBackend {
    Bundled(PathBuf, PathBuf), // (python_bin, script_path)
    System(String, PathBuf),   // (python_cmd, script_path)
}

fn resolve_backend() -> PythonBackend {
    // 1. Packaged production — Tauri $RESOURCE may be Contents/Resources (macOS) or exe_dir/resources (Win/Linux).
    //    With "resources/python/" + "resources/py-scripts/" the layout can be either $RESOURCE/python/... or
    //    $RESOURCE/resources/python/... depending on bundler map vs array. Probe both.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for base in [
                exe_dir.join("../Resources"),
                exe_dir.join("resources"),
                exe_dir.join("../../resources"),
            ] {
                let candidates: Vec<(PathBuf, PathBuf)> = if cfg!(windows) {
                    vec![
                        (base.join("python").join("python.exe"), base.join("py-scripts").join("extractor_cli.py")),
                        (base.join("resources").join("python").join("python.exe"), base.join("resources").join("py-scripts").join("extractor_cli.py")),
                    ]
                } else {
                    vec![
                        (base.join("python").join("bin").join("python3"), base.join("py-scripts").join("extractor_cli.py")),
                        (base.join("resources").join("python").join("bin").join("python3"), base.join("resources").join("py-scripts").join("extractor_cli.py")),
                    ]
                };
                for (py_bin, script) in candidates {
                    if py_bin.exists() && script.exists() {
                        return PythonBackend::Bundled(py_bin, script);
                    }
                }
            }
        }
    }

    // 2. Development fallback — host python3 + py-scripts/extractor_cli.py via CARGO_MANIFEST_DIR
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let dev_script = manifest_dir.join("../py-scripts/extractor_cli.py");
    if dev_script.exists() {
        return PythonBackend::System("python3".to_string(), dev_script);
    }
    // Fallback to py-scripts relative to CWD
    PythonBackend::System("python3".to_string(), PathBuf::from("py-scripts/extractor_cli.py"))
}

fn execute_cli(command: &str, args: &[String]) -> Option<Value> {
    let backend = resolve_backend();
    let mut cmd = match backend {
        PythonBackend::Bundled(py_bin, script) => {
            let mut c = std::process::Command::new(py_bin);
            c.arg(&script).arg(command);
            for a in args { c.arg(a); }
            c
        }
        PythonBackend::System(py_cmd, script) => {
            let mut c = std::process::Command::new(py_cmd);
            c.arg(&script).arg(command);
            for a in args { c.arg(a); }
            c
        }
    };
    let output = cmd.output().ok()?;
    if !output.status.success() {
        eprintln!("python extractor_cli error: {}", String::from_utf8_lossy(&output.stderr));
        return None;
    }
    let txt = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
        if v.get("_error").is_none() {
            return Some(v);
        } else {
            eprintln!("python extractor_cli returned _error: {}", txt.chars().take(1000).collect::<String>());
        }
    } else {
        eprintln!("python extractor_cli parse failed: {}", txt.chars().take(500).collect::<String>());
    }
    None
}

pub fn fetch_yahoo_extract_via_python(ticker: &str) -> Option<Value> {
    execute_cli("extract", &[ticker.to_string()])
}

pub fn fetch_yahoo_price_via_python(ticker: &str) -> Option<Value> {
    execute_cli("quote", &[ticker.to_string()])
}

pub fn fetch_yahoo_batch_via_python(tickers: &[String]) -> Option<Value> {
    execute_cli("batch", tickers)
}
