import { useRef, useEffect, useCallback } from "react";
import { WAVEFORM_SAMPLES } from "../utils/audioUtils";

interface AudioWaveformProps {
  /** Pre-computed waveform data (normalized 0-1) */
  waveformData?: number[];
  /** AnalyserNode for live visualization during recording */
  analyser?: AnalyserNode | null;
  /** Whether currently recording (enables live mode) */
  isRecording?: boolean;
  /** Current playback progress (0-1) */
  progress?: number;
  /** Height of the canvas */
  height?: number;
  /** Click handler for seeking */
  onClick?: (progress: number) => void;
  /** Custom color for the waveform bars */
  color?: string;
  /** Color for unplayed portion (when progress is set) */
  unplayedColor?: string;
  /** Whether the component is in an empty/disabled state */
  isEmpty?: boolean;
}

export const AudioWaveform = ({
  waveformData = [],
  analyser,
  isRecording = false,
  progress,
  height = 40,
  onClick,
  color,
  unplayedColor,
  isEmpty = false,
}: AudioWaveformProps) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);

  const drawWaveform = useCallback(
    (dataArray?: Uint8Array) => {
      const canvas = canvasRef.current;
      if (!canvas) return;

      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.scale(dpr, dpr);

      const width = rect.width;
      const canvasHeight = rect.height;

      ctx.clearRect(0, 0, width, canvasHeight);

      // get colors from CSS variables or props
      const computedStyle = getComputedStyle(canvas);
      const primaryColor =
        color ||
        computedStyle.getPropertyValue("--primary-color").trim() ||
        "#3b82f6";
      const secondaryColor =
        unplayedColor ||
        computedStyle.getPropertyValue("--node-border").trim() ||
        "#94a3b8";
      const emptyColor =
        computedStyle.getPropertyValue("--input-border").trim() || "#3f3f46";

      // live recording mode
      if (isRecording && dataArray) {
        const barWidth = width / WAVEFORM_SAMPLES;
        ctx.fillStyle = "#ef4444"; // Red for recording

        for (let i = 0; i < WAVEFORM_SAMPLES; i++) {
          const dataIndex = Math.floor(
            (i / WAVEFORM_SAMPLES) * dataArray.length,
          );
          const val = dataArray[dataIndex] / 255;
          const barHeight = Math.max(2, val * (canvasHeight - 4));
          const y = (canvasHeight - barHeight) / 2;
          ctx.fillRect(i * barWidth + 1, y, barWidth - 2, barHeight);
        }
        return;
      }

      // empty/placeholder state
      if (waveformData.length === 0 || isEmpty) {
        const barWidth = width / WAVEFORM_SAMPLES;
        ctx.fillStyle = emptyColor;
        for (let i = 0; i < WAVEFORM_SAMPLES; i++) {
          const x = i * barWidth;
          const barHeight = 4;
          const y = (canvasHeight - barHeight) / 2;
          ctx.fillRect(x + 1, y, barWidth - 2, barHeight);
        }
        return;
      }

      // static waveform with optional progress
      const barWidth = width / waveformData.length;

      waveformData.forEach((value, index) => {
        const x = index * barWidth;
        const barHeight = Math.max(2, value * (canvasHeight - 4));
        const y = (canvasHeight - barHeight) / 2;

        // color based on progress if provided
        if (progress !== undefined) {
          const barProgress = index / waveformData.length;
          ctx.fillStyle =
            barProgress < progress ? primaryColor : secondaryColor;
        } else {
          ctx.fillStyle = primaryColor;
        }

        ctx.fillRect(x + 1, y, barWidth - 2, barHeight);
      });
    },
    [waveformData, isRecording, progress, color, unplayedColor, isEmpty],
  );

  // live recording animation loop
  useEffect(() => {
    if (!isRecording || !analyser) {
      return;
    }

    const dataArray = new Uint8Array(analyser.frequencyBinCount);

    const animate = () => {
      analyser.getByteFrequencyData(dataArray);
      drawWaveform(dataArray);
      animationRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, [isRecording, analyser, drawWaveform]);

  // static waveform drawing
  useEffect(() => {
    if (!isRecording) {
      drawWaveform();
    }
  }, [drawWaveform, isRecording]);

  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!onClick) return;
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const clickProgress = x / rect.width;
    onClick(clickProgress);
  };

  return (
    <canvas
      ref={canvasRef}
      onClick={onClick ? handleClick : undefined}
      style={{
        width: "100%",
        height: `${height}px`,
        borderRadius: "4px",
        backgroundColor: "var(--node-header-bg)",
        cursor: onClick ? "pointer" : "default",
        opacity: isEmpty ? 0.5 : 1,
      }}
    />
  );
};
