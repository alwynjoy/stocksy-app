fn main() {
    println!("cargo:rerun-if-changed=../py-scripts/extractor_cli.py");
    println!("cargo:rerun-if-changed=../py-scripts/yahoo_stock_extract.py");
    tauri_build::build()
}
