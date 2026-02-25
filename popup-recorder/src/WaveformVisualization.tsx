import React, { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

interface AudioLevels {
  mic_level: number;
  system_level: number;
  mixed_level: number;
}

interface WaveformVisualizationProps {
  isRecording: boolean;
}

const WaveformVisualization: React.FC<WaveformVisualizationProps> = ({ isRecording }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationFrameRef = useRef<number>(0);
  const [audioLevels, setAudioLevels] = useState<AudioLevels>({
    mic_level: 0,
    system_level: 0,
    mixed_level: 0,
  });

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setupListener = async () => {
      try {
        unlisten = await listen<AudioLevels>('audio-levels', (event) => {
          setAudioLevels(event.payload);
        });
      } catch (error) {
        console.error('Failed to setup audio levels listener:', error);
      }
    };

    if (isRecording) {
      setupListener();
    }

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [isRecording]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      const { width, height } = canvas;

      // Clear canvas with transparent background
      ctx.clearRect(0, 0, width, height);

      if (!isRecording) {
        // Draw a flat line when not recording
        ctx.strokeStyle = 'rgba(255, 255, 255, 0.2)';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(0, height / 2);
        ctx.lineTo(width, height / 2);
        ctx.stroke();
        return;
      }

      const centerY = height / 2;
      const maxAmplitude = height * 0.35;
      const barWidth = 3;
      const barSpacing = 1;
      const numBars = Math.floor(width / (barWidth + barSpacing));

      const baseAmplitude = Math.min(audioLevels.mixed_level * maxAmplitude * 150, maxAmplitude);

      const time = Date.now() * 0.001;

      for (let i = 0; i < numBars; i++) {
        const x = i * (barWidth + barSpacing);

        const wave1 = Math.sin((i * 0.08) + time * 1.2);
        const wave2 = Math.sin((i * 0.03) + time * 0.8) * 0.6;
        const shapeWave = (wave1 + wave2) / 1.6;

        const spike = (Math.random() * 2 - 1);
        const combined = shapeWave * 0.35 + spike * 0.65;

        const amplitude = combined * baseAmplitude;

        const fadeDistance = Math.min(i, numBars - i) / (numBars * 0.08);
        const fadeFactor = Math.min(fadeDistance, 1);
        const finalAmplitude = amplitude * fadeFactor;

        const barHeight = Math.abs(finalAmplitude);
        const barY = centerY - barHeight / 2;

        const opacity = 0.2 + (barHeight / maxAmplitude) * 0.8;
        ctx.fillStyle = `rgba(255, 255, 255, ${opacity})`;

        ctx.fillRect(x, barY, barWidth, barHeight);
      }

      if (isRecording) {
        animationFrameRef.current = requestAnimationFrame(draw);
      }
    };

    draw();

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, [isRecording, audioLevels]);

  return (
    <canvas
      ref={canvasRef}
      width={540}
      height={54}
      data-tauri-drag-region
      style={{
        width: '100%',
        height: '54px',
      }}
    />
  );
};

export default WaveformVisualization;
