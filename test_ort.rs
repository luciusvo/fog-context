use std::path::Path;
fn main() {
    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".fog").join("models");
    let onnx_path = model_dir.join("all-MiniLM-L6-v2-q8.onnx");
    
    match ort::init()
            .with_name("fog-context-embed")
            .with_execution_providers([ort::ExecutionProvider::CPU(Default::default())])
            .commit() {
        Ok(_) => println!("Init OK"),
        Err(e) => println!("Init Error: {:?}", e),
    }

    let model = match ort::Session::builder() {
        Ok(b) => match b.with_optimization_level(ort::GraphOptimizationLevel::Level3) {
            Ok(b) => match b.with_intra_threads(1) {
                Ok(b) => match b.commit_from_file(&onnx_path) {
                    Ok(s) => s,
                    Err(e) => { println!("Commit Error: {:?}", e); return; },
                },
                Err(e) => { println!("Thread Error: {:?}", e); return; },
            },
            Err(e) => { println!("Opt Error: {:?}", e); return; },
        },
        Err(e) => { println!("Builder Error: {:?}", e); return; },
    };
    println!("Session loaded OK");
}
