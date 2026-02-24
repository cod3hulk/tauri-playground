use anyhow::Result;
use hound::{WavSpec, WavWriter};
use parking_lot::Mutex;
use screencapturekit::prelude::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

struct Recorder {
    stream: Option<SCStream>,
    file_path: Option<PathBuf>,
    writer: Option<Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>>,
}

pub struct AppState(Mutex<Recorder>);

impl AppState {
    pub fn new() -> Self {
        Self(Mutex::new(Recorder {
            stream: None,
            file_path: None,
            writer: None,
        }))
    }
}

struct AudioOutputHandler {
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
}

impl SCStreamOutputTrait for AudioOutputHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if let SCStreamOutputType::Audio = of_type {
            if let Some(buffer_list) = sample.audio_buffer_list() {
                let mut samples_to_write = Vec::new();
                
                // For ScreenCaptureKit audio, it's usually either:
                // 1. One buffer with interleaved samples (if number_channels > 1)
                // 2. Multiple buffers with one channel each (non-interleaved)
                
                let num_buffers = buffer_list.num_buffers();
                if num_buffers == 1 {
                    let buffer = buffer_list.get(0).unwrap();
                    let data = buffer.data();
                    let f32_samples: &[f32] = unsafe {
                        std::slice::from_raw_parts(
                            data.as_ptr() as *const f32,
                            data.len() / 4
                        )
                    };
                    samples_to_write.extend_from_slice(f32_samples);
                } else {
                    // Non-interleaved: multiple buffers. We need to interleave them for WAV.
                    // This is more complex, but let's handle the common case (1 buffer interleaved) first.
                    // If we have multiple, let's just take the first one for now or try to interleave.
                    let mut channel_data = Vec::new();
                    for i in 0..num_buffers {
                        let buffer = buffer_list.get(i).unwrap();
                        let data = buffer.data();
                        let f32_samples: &[f32] = unsafe {
                            std::slice::from_raw_parts(
                                data.as_ptr() as *const f32,
                                data.len() / 4
                            )
                        };
                        channel_data.push(f32_samples);
                    }
                    
                    if !channel_data.is_empty() {
                        let len = channel_data[0].len();
                        for i in 0..len {
                            for channel in &channel_data {
                                if i < channel.len() {
                                    samples_to_write.push(channel[i]);
                                }
                            }
                        }
                    }
                }

                if !samples_to_write.is_empty() {
                    let mut writer_lock = self.writer.lock();
                    if let Some(writer) = writer_lock.as_mut() {
                        for &s in &samples_to_write {
                            let _ = writer.write_sample(s);
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let mut recorder = state.0.lock();
    if recorder.stream.is_some() {
        return Err("Already recording".to_string());
    }

    let content = SCShareableContent::get().map_err(|e| e.to_string())?;
    let display = content
        .displays()
        .first()
        .cloned()
        .ok_or_else(|| "No display found".to_string())?;

    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();

    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48000)
        .with_channel_count(2);

    let audio_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    std::fs::create_dir_all(&audio_dir).map_err(|e| e.to_string())?;
    let file_path = audio_dir.join("system_audio.wav");

    let spec = WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let writer = WavWriter::create(&file_path, spec).map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let handler = AudioOutputHandler {
        writer: writer.clone(),
    };

    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(handler, SCStreamOutputType::Audio);
    stream.start_capture().map_err(|e| e.to_string())?;

    recorder.stream = Some(stream);
    recorder.file_path = Some(file_path.clone());
    recorder.writer = Some(writer);

    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn stop_recording(state: State<'_, AppState>) -> Result<String, String> {
    let mut recorder = state.0.lock();
    if let Some(stream) = recorder.stream.take() {
        stream.stop_capture().map_err(|e| e.to_string())?;
        
        if let Some(writer_arc) = recorder.writer.take() {
            let mut writer_lock = writer_arc.lock();
            if let Some(writer) = writer_lock.take() {
                writer.finalize().map_err(|e| e.to_string())?;
            }
        }

        if let Some(path) = &recorder.file_path {
            return Ok(path.to_string_lossy().to_string());
        }
    }
    Err("Not recording".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_recording, stop_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
