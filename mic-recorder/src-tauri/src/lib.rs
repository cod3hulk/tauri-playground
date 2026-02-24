use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use tauri::State;

pub struct AppState {
    pub stream: Arc<Mutex<Option<cpal::Stream>>>,
    pub recording_path: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
fn start_recording(state: State<'_, AppState>) -> Result<String, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;
    
    let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;

    let spec = hound::WavSpec {
        channels: config.channels() as u16,
        sample_rate: config.sample_rate(),
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("recorded_audio.wav");
    let path_str = path.to_string_lossy().to_string();
    
    let writer = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let writer_clone = writer.clone();
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if let Some(ref mut w) = *writer_clone.lock().unwrap() {
                for &sample in data {
                    w.write_sample(sample).ok();
                }
            }
        },
        move |err| {
            eprintln!("An error occurred on stream: {}", err);
        },
        None,
    ).map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    let mut state_stream = state.stream.lock().unwrap();
    *state_stream = Some(stream);

    let mut state_path = state.recording_path.lock().unwrap();
    *state_path = Some(path_str.clone());
    
    Ok(path_str)
}

#[tauri::command]
fn stop_recording(state: State<'_, AppState>) -> Result<String, String> {
    let mut state_stream = state.stream.lock().unwrap();
    if let Some(stream) = state_stream.take() {
        drop(stream);
        let path = state.recording_path.lock().unwrap().clone().unwrap_or_default();
        Ok(path)
    } else {
        Err("Not recording".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            stream: Arc::new(Mutex::new(None)),
            recording_path: Arc::new(Mutex::new(None)),
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_recording, stop_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
