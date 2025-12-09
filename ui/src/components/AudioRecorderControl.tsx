import { useState, useRef, useCallback, useEffect } from "react";
import { createPortal } from "react-dom";
import { Mic, Square, Upload, Trash2, Folder } from "lucide-react";
import { AudioWaveform } from "./AudioWaveform";
import { generateWaveformData } from "../utils/audioUtils";

// convert AudioBuffer to WAV format
function audioBufferToWav(buffer: AudioBuffer): ArrayBuffer {
  const numChannels = buffer.numberOfChannels;
  const sampleRate = buffer.sampleRate;
  const format = 1; // PCM
  const bitDepth = 16;

  const bytesPerSample = bitDepth / 8;
  const blockAlign = numChannels * bytesPerSample;
  const byteRate = sampleRate * blockAlign;
  const dataSize = buffer.length * blockAlign;
  const headerSize = 44;
  const totalSize = headerSize + dataSize;

  const arrayBuffer = new ArrayBuffer(totalSize);
  const view = new DataView(arrayBuffer);

  // write WAV header
  const writeString = (offset: number, str: string) => {
    for (let i = 0; i < str.length; i++) {
      view.setUint8(offset + i, str.charCodeAt(i));
    }
  };

  writeString(0, "RIFF");
  view.setUint32(4, totalSize - 8, true);
  writeString(8, "WAVE");
  writeString(12, "fmt ");
  view.setUint32(16, 16, true); // fmt chunk size
  view.setUint16(20, format, true);
  view.setUint16(22, numChannels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, byteRate, true);
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, bitDepth, true);
  writeString(36, "data");
  view.setUint32(40, dataSize, true);

  // interleave channels and write samples
  const channels: Float32Array[] = [];
  for (let i = 0; i < numChannels; i++) {
    channels.push(buffer.getChannelData(i));
  }

  let offset = 44;
  for (let i = 0; i < buffer.length; i++) {
    for (let ch = 0; ch < numChannels; ch++) {
      const sample = Math.max(-1, Math.min(1, channels[ch][i]));
      const intSample = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
      view.setInt16(offset, intSample, true);
      offset += 2;
    }
  }

  return arrayBuffer;
}

// convert blob to WAV format
async function convertToWav(blob: Blob): Promise<Blob> {
  const arrayBuffer = await blob.arrayBuffer();
  const audioContext = new AudioContext();
  const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);
  const wavBuffer = audioBufferToWav(audioBuffer);
  audioContext.close();
  return new Blob([wavBuffer], { type: "audio/wav" });
}

interface AudioRecorderControlProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  onFocus?: () => void;
}

export const AudioRecorderControl = ({
  value,
  onChange,
  disabled,
  onFocus,
}: AudioRecorderControlProps) => {
  const [isRecording, setIsRecording] = useState(false);
  const [recordingTime, setRecordingTime] = useState(0);
  const [waveformData, setWaveformData] = useState<number[]>([]);
  const [analyser, setAnalyser] = useState<AnalyserNode | null>(null);

  const [showAssetSelector, setShowAssetSelector] = useState(false);
  const [assets, setAssets] = useState<string[]>([]);
  const [selectorPosition, setSelectorPosition] = useState({ top: 0, left: 0 });
  const buttonRef = useRef<HTMLButtonElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // derive hasAudio from value instead of storing in state
  const hasAudio = !!value;

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const audioContextRef = useRef<AudioContext | null>(null);

  // generate waveform when value changes
  useEffect(() => {
    if (
      value &&
      (value.startsWith("data:") || value.startsWith("/api/assets/"))
    ) {
      generateWaveformData(value).then(setWaveformData);
    } else {
      // clear waveform when value is removed - this synchronous setState is intentional
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setWaveformData([]);
    }
  }, [value]);

  // close selector when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      const clickedButton = buttonRef.current?.contains(target);
      const clickedDropdown = dropdownRef.current?.contains(target);
      if (!clickedButton && !clickedDropdown) {
        setShowAssetSelector(false);
      }
    };

    if (showAssetSelector) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [showAssetSelector]);

  const uploadFile = async (blob: Blob, filename: string) => {
    const formData = new FormData();
    formData.append("file", blob, filename);

    try {
      const res = await fetch("/api/assets/upload", {
        method: "POST",
        body: formData,
      });
      if (res.ok) {
        const data = await res.json();
        return data.url;
      }
    } catch (e) {
      console.error("Upload failed", e);
    }
    return null;
  };

  const fetchAssets = async () => {
    try {
      const res = await fetch("/api/assets/list");
      if (res.ok) {
        const data = await res.json();
        // filter for audio files
        const audioFiles = data.filter(
          (f: string) =>
            f.endsWith(".wav") ||
            f.endsWith(".mp3") ||
            f.endsWith(".ogg") ||
            f.endsWith(".webm"),
        );
        setAssets(audioFiles);
      }
    } catch (e) {
      console.error("Failed to fetch assets", e);
    }
  };

  const toggleSelector = () => {
    if (!showAssetSelector) {
      const rect = buttonRef.current?.getBoundingClientRect();
      if (rect) {
        setSelectorPosition({
          top: rect.bottom + 4,
          left: rect.left,
        });
      }
      fetchAssets();
    }
    setShowAssetSelector(!showAssetSelector);
  };

  const handleAssetSelect = (asset: string) => {
    onChange(`/api/assets/${asset}`);
    setShowAssetSelector(false);
  };

  const startRecording = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });

      // set up audio context for visualization
      const audioContext = new AudioContext();
      const newAnalyser = audioContext.createAnalyser();
      const source = audioContext.createMediaStreamSource(stream);
      source.connect(newAnalyser);
      newAnalyser.fftSize = 256;
      setAnalyser(newAnalyser);
      audioContextRef.current = audioContext;

      const mediaRecorder = new MediaRecorder(stream);
      mediaRecorderRef.current = mediaRecorder;
      chunksRef.current = [];

      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          chunksRef.current.push(e.data);
        }
      };

      mediaRecorder.onstop = async () => {
        const blob = new Blob(chunksRef.current, { type: "audio/webm" });

        // convert to WAV format for better compatibility with STT servers
        const wavBlob = await convertToWav(blob);
        const url = await uploadFile(wavBlob, "recording.wav");
        if (url) {
          onChange(url);
        }

        // clean up
        stream.getTracks().forEach((track) => track.stop());
        if (audioContextRef.current) {
          audioContextRef.current.close();
          audioContextRef.current = null;
        }
        setAnalyser(null);
      };

      mediaRecorder.start();
      setIsRecording(true);
      setRecordingTime(0);

      // start timer
      timerRef.current = window.setInterval(() => {
        setRecordingTime((t) => t + 1);
      }, 1000);
    } catch (err) {
      console.error("Error accessing microphone:", err);
    }
  }, [onChange]);

  const stopRecording = useCallback(() => {
    if (mediaRecorderRef.current && isRecording) {
      mediaRecorderRef.current.stop();
      setIsRecording(false);
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    }
  }, [isRecording]);

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    let blob: Blob = file;
    if (file.type !== "audio/wav" && file.type !== "audio/wave") {
      try {
        blob = await convertToWav(file);
      } catch (err) {
        console.error("Conversion to WAV failed, uploading original:", err);
      }
    }

    const url = await uploadFile(
      blob,
      file.name.replace(/\.[^/.]+$/, "") + ".wav",
    );
    if (url) {
      onChange(url);
    }
  };

  const clearAudio = () => {
    onChange("");
    setWaveformData([]);
  };

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  // cleanup on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
      }
      if (audioContextRef.current) {
        audioContextRef.current.close();
      }
    };
  }, []);

  return (
    <div
      className="nodrag"
      onFocus={onFocus}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        padding: "8px",
        backgroundColor: "var(--input-bg)",
        borderRadius: "4px",
        border: "1px solid var(--input-border)",
      }}
    >
      {/* Waveform preview */}
      <AudioWaveform
        waveformData={waveformData}
        analyser={analyser}
        isRecording={isRecording}
        height={40}
        isEmpty={!hasAudio && !isRecording}
      />

      {/* Controls */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          position: "relative",
        }}
      >
        {/* Record button */}
        <button
          onClick={isRecording ? stopRecording : startRecording}
          disabled={disabled}
          type="button"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "32px",
            height: "32px",
            border: "none",
            borderRadius: "50%",
            backgroundColor: isRecording ? "#ef4444" : "var(--primary-color)",
            color: "white",
            cursor: disabled ? "not-allowed" : "pointer",
            opacity: disabled ? 0.5 : 1,
          }}
          title={isRecording ? "Stop recording" : "Start recording"}
        >
          {isRecording ? <Square size={14} /> : <Mic size={16} />}
        </button>

        {/* Upload button */}
        <button
          onClick={() => fileInputRef.current?.click()}
          disabled={disabled || isRecording}
          type="button"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "28px",
            height: "28px",
            border: "1px solid var(--input-border)",
            borderRadius: "4px",
            backgroundColor: "var(--button-bg)",
            color: "var(--text-color)",
            cursor: disabled || isRecording ? "not-allowed" : "pointer",
            opacity: disabled || isRecording ? 0.5 : 1,
          }}
          title="Upload audio file"
        >
          <Upload size={14} />
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept="audio/*"
          onChange={handleFileUpload}
          style={{ display: "none" }}
        />

        {/* Asset Selector Button */}
        <button
          ref={buttonRef}
          onClick={toggleSelector}
          disabled={disabled || isRecording}
          type="button"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "28px",
            height: "28px",
            border: "1px solid var(--input-border)",
            borderRadius: "4px",
            backgroundColor: showAssetSelector
              ? "var(--accent-color)"
              : "var(--button-bg)",
            color: showAssetSelector ? "white" : "var(--text-color)",
            cursor: disabled || isRecording ? "not-allowed" : "pointer",
            opacity: disabled || isRecording ? 0.5 : 1,
          }}
          title="Select from assets"
        >
          <Folder size={14} />
        </button>

        {showAssetSelector &&
          createPortal(
            <div
              ref={dropdownRef}
              style={{
                position: "fixed",
                top: selectorPosition.top,
                left: selectorPosition.left,
                background: "var(--panel-bg)",
                border: "1px solid var(--panel-border)",
                borderRadius: "4px",
                boxShadow: "0 4px 6px -1px rgba(0,0,0,0.1)",
                zIndex: 9999,
                minWidth: "160px",
                maxHeight: "200px",
                overflowY: "auto",
              }}
            >
              {assets.length === 0 ? (
                <div
                  style={{
                    padding: "8px",
                    fontSize: "12px",
                    color: "var(--text-muted)",
                  }}
                >
                  No audio assets found
                </div>
              ) : (
                assets.map((asset) => (
                  <div
                    key={asset}
                    onClick={() => handleAssetSelect(asset)}
                    style={{
                      padding: "6px 10px",
                      cursor: "pointer",
                      fontSize: "12px",
                      color: "var(--text-color)",
                      borderBottom: "1px solid var(--panel-border-subtle)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                    className="hover:bg-accent hover:text-white"
                  >
                    {asset}
                  </div>
                ))
              )}
            </div>,
            document.body,
          )}

        {/* Clear button */}
        {hasAudio && !isRecording && (
          <button
            onClick={clearAudio}
            disabled={disabled}
            type="button"
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "28px",
              height: "28px",
              border: "1px solid var(--input-border)",
              borderRadius: "4px",
              backgroundColor: "var(--button-bg)",
              color: "var(--danger-color)",
              cursor: disabled ? "not-allowed" : "pointer",
              opacity: disabled ? 0.5 : 1,
            }}
            title="Clear audio"
          >
            <Trash2 size={14} />
          </button>
        )}

        {/* Status text */}
        <span
          style={{
            fontSize: "11px",
            color: isRecording ? "#ef4444" : "var(--node-border)",
            fontFamily: "monospace",
          }}
        >
          {isRecording
            ? `Recording ${formatTime(recordingTime)}`
            : hasAudio
              ? "Audio ready"
              : "No audio"}
        </span>
      </div>
    </div>
  );
};
