import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
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

  return (
    <div className="container" data-tauri-drag-region>
      <div className="pill" data-tauri-drag-region>
        <WaveformVisualization isRecording={isRecording} />
      </div>
    </div>
  );
}

export default App;
