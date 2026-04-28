//! fog-mcp-server/src/semantic.rs
//! Handles ONNX semantic search embedding via ort + tokenizers.
//! Conditionally compiled with #[cfg(feature = "embedding")]

use std::sync::OnceLock;
use std::path::Path;

use ort::session::{Session, builder::GraphOptimizationLevel};
use tokenizers::Tokenizer;
use ndarray::Array2;

pub struct SemanticModel {
    session: std::sync::Mutex<Session>,
    tokenizer: Tokenizer,
}

static MODEL: OnceLock<Option<SemanticModel>> = OnceLock::new();
static ORT_INIT: OnceLock<()> = OnceLock::new();

pub fn get_model() -> Option<&'static SemanticModel> {
    MODEL.get_or_init(|| {
        load_model().unwrap_or_else(|e| {
            eprintln!("ERROR: Failed to load semantic model: {:?}", e);
            None
        })
    }).as_ref()
}

fn load_model() -> anyhow::Result<Option<SemanticModel>> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
    let model_dir = Path::new(&home).join(".fog").join("models");

    let onnx_path = model_dir.join("all-MiniLM-L6-v2-q8.onnx");
    if !onnx_path.exists() {
        tracing::info!("Semantic model not found at {}. Skipping ONNX init.", onnx_path.display());
        return Ok(None);
    }

    ORT_INIT.get_or_init(|| {
        let _ = ort::init()
            .with_name("fog-context")
            .commit();
    });

    let session = Session::builder()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .commit_from_file(&onnx_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let tokenizer_path = model_dir.join("tokenizer.json");
    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    Ok(Some(SemanticModel { session: std::sync::Mutex::new(session), tokenizer }))
}

pub fn embed_text(text: &str) -> anyhow::Result<Vec<f32>> {
    let model = get_model().ok_or_else(|| anyhow::anyhow!("Model not loaded"))?;
    
    let encoding = model.tokenizer.encode(text, true)
        .map_err(|e| anyhow::anyhow!("Tokenize error: {}", e))?;
        
    let input_ids = encoding.get_ids().iter().map(|&x| x as i64).collect::<Vec<_>>();
    let attention_mask = encoding.get_attention_mask().iter().map(|&x| x as i64).collect::<Vec<_>>();
    let token_type_ids = encoding.get_type_ids().iter().map(|&x| x as i64).collect::<Vec<_>>();
    
    let seq_len = input_ids.len();
    
    let input_ids_array = Array2::from_shape_vec((1, seq_len), input_ids)?;
    let attention_mask_array = Array2::from_shape_vec((1, seq_len), attention_mask)?;
    let token_type_ids_array = Array2::from_shape_vec((1, seq_len), token_type_ids)?;

    let input_ids_tensor = ort::value::Tensor::from_array(input_ids_array)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let attention_mask_tensor = ort::value::Tensor::from_array(attention_mask_array.clone())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let token_type_ids_tensor = ort::value::Tensor::from_array(token_type_ids_array)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let inputs = ort::inputs![
        "input_ids" => input_ids_tensor,
        "attention_mask" => attention_mask_tensor,
        "token_type_ids" => token_type_ids_tensor,
    ];
    
    let mut session = model.session.lock().unwrap();
    let outputs: ort::session::SessionOutputs = session.run(inputs)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let output_val: &ort::value::DynValue = &outputs["last_hidden_state"];
    let (shape_vec, data) = output_val.try_extract_tensor::<f32>()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    
    let hidden_size = shape_vec[2] as usize;
    
    let mut mean_pool = vec![0.0f32; hidden_size];
    let mut sum_mask = 0.0f32;
    
    let mask_view = attention_mask_array.view();
    
    for i in 0..seq_len {
        let mask_val = mask_view[[0, i]] as f32;
        sum_mask += mask_val;
        for j in 0..hidden_size {
            mean_pool[j] += data[i * hidden_size + j] * mask_val;
        }
    }
    
    if sum_mask > 0.0 {
        for j in 0..hidden_size {
            mean_pool[j] /= sum_mask;
        }
    }
    
    let norm: f32 = mean_pool.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for j in 0..hidden_size {
            mean_pool[j] /= norm;
        }
    }

    Ok(mean_pool)
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
