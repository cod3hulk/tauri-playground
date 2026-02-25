import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import WaveformVisualization from "./WaveformVisualization";

function App() {
  const [isRecording, setIsRecording] = useState(false);

  useEffect(() => {
    const promise = listen<{ recording: boolean }>("recording-state", (e) => {
      setIsRecording(e.payload.recording);
    });
    return () => {
      promise.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        invoke("cancel_recording").catch(console.error);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <div className="container" data-tauri-drag-region>
      <div className="pill" data-tauri-drag-region>
        <div className="waveform-area" data-tauri-drag-region>
          <WaveformVisualization isRecording={isRecording} />
        </div>
        <div className="controls">
          <div className="controls-right">
            <span className="control-label">Stop</span>
            <kbd className="key">⇧⌘</kbd>
            <kbd className="key key-accent">R</kbd>
            <span className="control-label cancel-label">Cancel</span>
            <kbd className="key">esc</kbd>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
