import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [filePath, setFilePath] = useState("");
  const [error, setError] = useState("");

  async function startRecording() {
    try {
      setError("");
      const path = await invoke<string>("start_recording");
      setIsRecording(true);
      setFilePath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function stopRecording() {
    try {
      const path = await invoke<string>("stop_recording");
      setIsRecording(false);
      setFilePath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <main className="container">
      <h1>System Audio Recorder</h1>

      <div className="card">
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

      {isRecording && <p className="recording-status">🔴 Recording...</p>}

      {filePath && (
        <div className="file-info">
          <p>File saved at:</p>
          <code>{filePath}</code>
        </div>
      )}

      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
