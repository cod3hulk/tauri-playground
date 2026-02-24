use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use parking_lot::Mutex;
use screencapturekit::prelude::*;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use chrono::Local;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, PanelBuilder, PanelLevel};

#[cfg(target_os = "macos")]
tauri_panel!(panel!(RecordingOverlayPanel {
    config: {
        can_become_key_window: true,
        is_floating_panel: true,
        hides_on_deactivate: false,
        responds_to_keyboard_events: true
    }
}));

struct SharedRecorder {
    system_stream: Option<SCStream>,
    mic_stream: Option<cpal::Stream>,
    file_path: Option<PathBuf>,
    writer: Option<Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>>,
    
    // Buffers for mixing
    system_buffer: Arc<Mutex<VecDeque<f32>>>,
    mic_buffer: Arc<Mutex<VecDeque<f32>>>,
}

pub struct AppState(Mutex<SharedRecorder>);

impl AppState {
    pub fn new() -> Self {
        Self(Mutex::new(SharedRecorder {
            system_stream: None,
            mic_stream: None,
            file_path: None,
            writer: None,
            system_buffer: Arc::new(Mutex::new(VecDeque::new())),
            mic_buffer: Arc::new(Mutex::new(VecDeque::new())),
        }))
    }

    pub fn is_recording(&self) -> bool {
        let recorder = self.0.lock();
        recorder.system_stream.is_some() || recorder.mic_stream.is_some()
    }
}

struct Mixer {
    system_buffer: Arc<Mutex<VecDeque<f32>>>,
    mic_buffer: Arc<Mutex<VecDeque<f32>>>,
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
}

impl Mixer {
    fn mix_available(&self) {
        let mut sys = self.system_buffer.lock();
        let mut mic = self.mic_buffer.lock();
        let mut writer_lock = self.writer.lock();
        
        if let Some(writer) = writer_lock.as_mut() {
            // We assume stereo (2 channels) for output
            while sys.len() >= 2 && mic.len() >= 2 {
                let s1 = sys.pop_front().unwrap();
                let s2 = sys.pop_front().unwrap();
                let m1 = mic.pop_front().unwrap();
                let m2 = mic.pop_front().unwrap();
                
                // Simple mixing: average the samples
                let mixed_1 = (s1 + m1) / 2.0;
                let mixed_2 = (s2 + m2) / 2.0;
                
                let _ = writer.write_sample(mixed_1);
                let _ = writer.write_sample(mixed_2);
            }
        }
    }
}

// Handler for ScreenCaptureKit (System Audio)
struct SystemAudioOutputHandler {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    mixer_trigger: Arc<Mixer>,
}

impl SCStreamOutputTrait for SystemAudioOutputHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if let SCStreamOutputType::Audio = of_type {
            if let Some(buffer_list) = sample.audio_buffer_list() {
                let mut samples = Vec::new();
                let num_buffers = buffer_list.num_buffers();
                
                if num_buffers == 1 {
                    let buffer = buffer_list.get(0).unwrap();
                    let data = buffer.data();
                    let f32_samples: &[f32] = unsafe {
                        std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
                    };
                    samples.extend_from_slice(f32_samples);
                } else {
                    let mut channel_data = Vec::new();
                    for i in 0..num_buffers {
                        let buffer = buffer_list.get(i).unwrap();
                        let data = buffer.data();
                        let f32_samples: &[f32] = unsafe {
                            std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
                        };
                        channel_data.push(f32_samples);
                    }
                    if !channel_data.is_empty() {
                        let len = channel_data[0].len();
                        for i in 0..len {
                            for channel in &channel_data {
                                if i < channel.len() {
                                    samples.push(channel[i]);
                                }
                            }
                        }
                    }
                }

                if !samples.is_empty() {
                    self.buffer.lock().extend(samples);
                    self.mixer_trigger.mix_available();
                }
            }
        }
    }
}

fn update_overlay(app: &AppHandle, is_recording: bool) {
    let _ = app.emit("recording-status", is_recording);
    if let Some(window) = app.get_webview_window("overlay") {
        if is_recording {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    }
}

#[tauri::command]
async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let mut recorder = state.0.lock();
    if recorder.system_stream.is_some() || recorder.mic_stream.is_some() {
        return Err("Already recording".to_string());
    }

    // --- SETUP WAV WRITER ---
    let audio_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    std::fs::create_dir_all(&audio_dir).map_err(|e| e.to_string())?;
    
    // Use timestamp in filename
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let file_path = audio_dir.join(format!("recording_{}.wav", timestamp));

    let spec = WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let writer = WavWriter::create(&file_path, spec).map_err(|e| e.to_string())?;
    let writer_arc = Arc::new(Mutex::new(Some(writer)));
    
    let mixer = Arc::new(Mixer {
        system_buffer: recorder.system_buffer.clone(),
        mic_buffer: recorder.mic_buffer.clone(),
        writer: writer_arc.clone(),
    });

    // --- SETUP SYSTEM AUDIO (ScreenCaptureKit) ---
    let content = SCShareableContent::get().map_err(|e| e.to_string())?;
    let display = content.displays().first().cloned().ok_or_else(|| "No display found".to_string())?;
    let filter = SCContentFilter::create().with_display(&display).with_excluding_windows(&[]).build();
    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48000)
        .with_channel_count(2);

    let system_handler = SystemAudioOutputHandler {
        buffer: recorder.system_buffer.clone(),
        mixer_trigger: mixer.clone(),
    };

    let mut system_stream = SCStream::new(&filter, &config);
    system_stream.add_output_handler(system_handler, SCStreamOutputType::Audio);
    system_stream.start_capture().map_err(|e| e.to_string())?;

    // --- SETUP MIC AUDIO (cpal) ---
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No input device available")?;
    
    let supported_configs = device.supported_input_configs()
        .map_err(|e| e.to_string())?;
    
    // --- MIC CONFIGURATION ---
    let mic_config_support = supported_configs
        .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
        .find(|c| c.min_sample_rate() <= 48000 && c.max_sample_rate() >= 48000)
        .or_else(|| device.supported_input_configs().ok()?.next())
        .ok_or("Could not find any suitable input config")?;
    
    let mic_channels = mic_config_support.channels();
    let mic_source_sr = if mic_config_support.min_sample_rate() <= 48000 && mic_config_support.max_sample_rate() >= 48000 {
        48000
    } else {
        mic_config_support.max_sample_rate()
    };
    
    let mic_config = mic_config_support.with_sample_rate(mic_source_sr);
    eprintln!("Selected Mic: {} channels, {} Hz", mic_channels, mic_source_sr);

    let mic_buffer_clone = recorder.mic_buffer.clone();
    let mixer_clone = mixer.clone();
    
    // Resampling state for nearest-neighbor interpolation
    let mut total_in = 0u64;
    let mut total_out = 0u64;
    let target_sr_val = 48000.0f64;
    let source_sr_val = mic_source_sr as f64;

    let mic_stream = device.build_input_stream(
        &mic_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut mic_buf = mic_buffer_clone.lock();
            for frame in data.chunks(mic_channels as usize) {
                total_in += 1;
                // Resample to 48000 Hz by repeating or skipping samples
                while (total_out as f64 * source_sr_val) < (total_in as f64 * target_sr_val) {
                    if mic_channels == 1 {
                        mic_buf.push_back(frame[0]);
                        mic_buf.push_back(frame[0]);
                    } else if mic_channels >= 2 {
                        mic_buf.push_back(frame[0]);
                        mic_buf.push_back(frame[1]);
                    }
                    total_out += 1;
                }
            }
            drop(mic_buf);
            mixer_clone.mix_available();
        },
        move |err| {
            eprintln!("Mic stream error: {}", err);
        },
        None,
    ).map_err(|e| e.to_string())?;

    mic_stream.play().map_err(|e| e.to_string())?;

    recorder.system_stream = Some(system_stream);
    recorder.mic_stream = Some(mic_stream);
    recorder.file_path = Some(file_path.clone());
    recorder.writer = Some(writer_arc);

    update_overlay(&app, true);

    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let mut recorder = state.0.lock();
    
    if let Some(stream) = recorder.system_stream.take() {
        let _ = stream.stop_capture();
    }
    
    if let Some(stream) = recorder.mic_stream.take() {
        let _ = stream.pause(); 
    }

    if let Some(writer_arc) = recorder.writer.take() {
        let mut writer_lock = writer_arc.lock();
        if let Some(writer) = writer_lock.take() {
            writer.finalize().map_err(|e| e.to_string())?;
        }
    }

    // Clear buffers
    recorder.system_buffer.lock().clear();
    recorder.mic_buffer.lock().clear();

    update_overlay(&app, false);

    if let Some(path) = &recorder.file_path {
        return Ok(path.to_string_lossy().to_string());
    }
    
    Err("Not recording".to_string())
}

#[tauri::command]
async fn toggle_recording(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let is_recording = state.is_recording();
    if is_recording {
        stop_recording(app, state).await?;
        Ok(false)
    } else {
        start_recording(app, state).await?;
        Ok(true)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .manage(AppState::new())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let ctrl_shift_r = if cfg!(target_os = "macos") {
                "Command+Shift+R"
            } else {
                "Ctrl+Shift+R"
            };

            let shortcut = ctrl_shift_r.parse::<Shortcut>().unwrap();

            app.global_shortcut().on_shortcut(shortcut, move |app_handle, scut, event| {
                if event.state == ShortcutState::Pressed && scut == &shortcut {
                    let app_handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let app_handle_inner = app_handle.clone();
                        let state = app_handle.state::<AppState>();
                        let _ = toggle_recording(app_handle_inner, state).await;
                    });
                }
            })?;

            // Create Overlay Window / Panel
            #[cfg(target_os = "macos")]
            {
                let panel = PanelBuilder::<_, RecordingOverlayPanel>::new(app.handle(), "overlay")
                    .url(WebviewUrl::App("/?overlay=true".into()))
                    .level(PanelLevel::Status)
                    .size(tauri::Size::Logical(tauri::LogicalSize {
                        width: 540.0,
                        height: 260.0,
                    }))
                    .has_shadow(false)
                    .transparent(true)
                    .corner_radius(0.0)
                    .with_window(|w| {
                        w.decorations(false)
                         .transparent(true)
                         .shadow(false)
                         .accept_first_mouse(true)
                    })
                    .build()?;
                
                if let Some(window) = app.get_webview_window("overlay") {
                    let _ = window.center();
                }
                let _ = panel.hide();
            }

            #[cfg(not(target_os = "macos"))]
            {
                use tauri::WebviewWindowBuilder;
                let _overlay = WebviewWindowBuilder::new(
                    app,
                    "overlay",
                    WebviewUrl::App("/?overlay=true".into())
                )
                .title("Recording Overlay")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .inner_size(540.0, 260.0)
                .visible(false)
                .resizable(false)
                .skip_taskbar(true)
                .accept_first_mouse(true)
                .build()
                .unwrap();
                
                let _ = _overlay.center();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![start_recording, stop_recording, toggle_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
