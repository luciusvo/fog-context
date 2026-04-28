use std::path::Path;
fn main() {
    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".fog").join("models");
    let onnx_path = model_dir.join("all-MiniLM-L6-v2-q8.onnx");
    println!("Path: {}", onnx_path.display());
    println!("Exists: {}", onnx_path.exists());
}
