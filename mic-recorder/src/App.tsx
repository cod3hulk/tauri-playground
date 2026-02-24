import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [status, setStatus] = useState("Ready");
  const [filePath, setFilePath] = useState("");

  async function toggleRecording() {
    try {
      if (isRecording) {
        const path = await invoke<string>("stop_recording");
        setStatus("Stopped");
        setFilePath(path);
        setIsRecording(false);
      } else {
        const path = await invoke<string>("start_recording");
        setStatus("Recording...");
        setFilePath(path);
        setIsRecording(true);
      }
    } catch (error) {
      console.error(error);
      setStatus(`Error: ${error}`);
    }
  }

  return (
    <main className="container">
      <h1>Mic Recorder</h1>

      <div className="card">
        <p>Status: <strong>{status}</strong></p>
        
        <button 
          onClick={toggleRecording}
          className={isRecording ? "recording" : ""}
        >
          {isRecording ? "Stop Recording" : "Start Recording"}
        </button>

        {filePath && (
          <div className="file-info">
            <p className="label">Save Path:</p>
            <code className="path">{filePath}</code>
          </div>
        )}
      </div>

      <p className="hint">
        Recordings are saved to your system's temporary folder.
      </p>
    </main>
  );
}

export default App;
