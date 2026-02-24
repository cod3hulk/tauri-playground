import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [filePath, setFilePath] = useState("");
  const [error, setError] = useState("");
  const [status, setStatus] = useState("Ready");

  async function startRecording() {
    try {
      setError("");
      setStatus("Starting...");
      const path = await invoke<string>("start_recording");
      setIsRecording(true);
      setFilePath(path);
      setStatus("Recording Mic + System Audio");
    } catch (e) {
      setError(String(e));
      setStatus("Error");
    }
  }

  async function stopRecording() {
    try {
      setStatus("Stopping...");
      const path = await invoke<string>("stop_recording");
      setIsRecording(false);
      setFilePath(path);
      setStatus("Saved");
    } catch (e) {
      setError(String(e));
      setStatus("Error");
    }
  }

  return (
    <main className="container">
      <h1>Combined Recorder</h1>
      <p className="description">Captures Microphone and System Audio into one file.</p>

      <div className="card">
        <div className="status-badge">
          <span className={`dot ${isRecording ? "active" : ""}`}></span>
          {status}
        </div>
        
        {isRecording ? (
          <button onClick={stopRecording} className="stop-btn">
            Stop Recording
          </button>
        ) : (
          <button onClick={startRecording} className="start-btn">
            Start Recording
          </button>
        )}
      </div>

      {filePath && (
        <div className="file-info fade-in">
          <p className="label">Latest Recording:</p>
          <code className="path">{filePath}</code>
        </div>
      )}

      {error && <div className="error-box">{error}</div>}
      
      <div className="info-footer">
        <p>Uses ScreenCaptureKit & CPAL</p>
        <p>Mixed at 48kHz Stereo</p>
      </div>
    </main>
  );
}

export default App;
