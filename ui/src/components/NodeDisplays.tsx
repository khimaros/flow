import { useRef, useEffect, useState, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import {
  JsonView,
  darkStyles,
  defaultStyles,
  allExpanded,
} from "react-json-view-lite";
import "react-json-view-lite/dist/index.css";
import { Play, Pause, Square, Download } from "lucide-react";
import { AudioWaveform } from "./AudioWaveform";
import { generateWaveformData } from "../utils/audioUtils";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type NodeData = any;

// extract URL and path from node data, checking outputs first, then inputs
function extractFileData(
  data: NodeData,
  outputKey: string,
  inputKey: string,
): { url?: string; path?: string } {
  // check outputs first (from execution)
  const outputData = data.outputs?.[outputKey];
  if (outputData?.url) {
    return { url: outputData.url, path: outputData.path };
  }

  // fall back to inputs (from saved workflow)
  const inputData = data[inputKey];
  if (!inputData) return {};

  try {
    const parsed =
      typeof inputData === "string" ? JSON.parse(inputData) : inputData;
    return { url: parsed?.url, path: parsed?.path };
  } catch {
    return {};
  }
}

export const DisplayAudioNode = ({ data }: { data: NodeData }) => {
  const { url: audioUrl, path: audioPath } = extractFileData(
    data,
    "audio",
    "audio",
  );

  const audioRef = useRef<HTMLAudioElement>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const sourceRef = useRef<MediaElementAudioSourceNode | null>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [waveformData, setWaveformData] = useState<number[]>([]);

  // generate waveform data when URL changes
  useEffect(() => {
    if (audioUrl) {
      generateWaveformData(audioUrl).then(setWaveformData);
    } else {
      // clear waveform when URL is removed - this synchronous setState is intentional
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setWaveformData([]);
    }
  }, [audioUrl]);

  // initialize audio context for playback
  const initAudioContext = useCallback(() => {
    if (!audioRef.current || audioContextRef.current) return;

    const audioContext = new AudioContext();
    const source = audioContext.createMediaElementSource(audioRef.current);
    source.connect(audioContext.destination);

    audioContextRef.current = audioContext;
    sourceRef.current = source;
  }, []);

  // update time display
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const updateTime = () => setCurrentTime(audio.currentTime);
    const updateDuration = () => setDuration(audio.duration);
    const handleEnded = () => setIsPlaying(false);

    audio.addEventListener("timeupdate", updateTime);
    audio.addEventListener("loadedmetadata", updateDuration);
    audio.addEventListener("ended", handleEnded);

    return () => {
      audio.removeEventListener("timeupdate", updateTime);
      audio.removeEventListener("loadedmetadata", updateDuration);
      audio.removeEventListener("ended", handleEnded);
    };
  }, [audioUrl]);

  // cleanup
  useEffect(() => {
    const audioContext = audioContextRef.current;
    return () => {
      if (audioContext) {
        audioContext.close();
      }
    };
  }, []);

  const togglePlay = () => {
    const audio = audioRef.current;
    if (!audio) return;

    if (!audioContextRef.current) {
      initAudioContext();
    }

    if (isPlaying) {
      audio.pause();
    } else {
      audio.play();
    }
    setIsPlaying(!isPlaying);
  };

  const stop = () => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.pause();
    audio.currentTime = 0;
    setIsPlaying(false);
  };

  const handleSeek = (progress: number) => {
    const audio = audioRef.current;
    if (!audio || !duration) return;
    audio.currentTime = progress * duration;
  };

  const formatTime = (time: number) => {
    if (isNaN(time)) return "0:00";
    const mins = Math.floor(time / 60);
    const secs = Math.floor(time % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const handleDownload = () => {
    if (!audioUrl) return;
    const link = document.createElement("a");
    link.href = audioUrl;
    // extract basename from path
    const filename =
      audioPath?.split("/").pop() || audioPath?.split("\\").pop();
    if (filename) {
      link.download = filename;
    }
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  };

  const hasAudio = !!audioUrl;
  const progress = duration > 0 ? currentTime / duration : 0;

  return (
    <div
      className="nodrag"
      style={{
        marginTop: "8px",
        width: "100%",
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        backgroundColor: "var(--node-header-bg)",
        borderRadius: "4px",
        padding: "12px",
        boxSizing: "border-box",
      }}
    >
      {audioUrl && <audio ref={audioRef} src={audioUrl} preload="metadata" />}

      {/* Waveform */}
      <AudioWaveform
        waveformData={waveformData}
        progress={hasAudio ? progress : undefined}
        height={60}
        onClick={hasAudio ? handleSeek : undefined}
        isEmpty={!hasAudio}
      />

      {/* Controls */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
        }}
      >
        <button
          onClick={togglePlay}
          disabled={!hasAudio}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "32px",
            height: "32px",
            border: "none",
            borderRadius: "50%",
            backgroundColor: "var(--primary-color)",
            color: "white",
            cursor: hasAudio ? "pointer" : "not-allowed",
            opacity: hasAudio ? 1 : 0.5,
          }}
        >
          {isPlaying ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <button
          onClick={stop}
          disabled={!hasAudio}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "28px",
            height: "28px",
            border: "1px solid var(--input-border)",
            borderRadius: "4px",
            backgroundColor: "var(--input-bg)",
            color: "var(--text-color)",
            cursor: hasAudio ? "pointer" : "not-allowed",
            opacity: hasAudio ? 1 : 0.5,
          }}
        >
          <Square size={12} />
        </button>
        <button
          onClick={handleDownload}
          disabled={!hasAudio}
          title="Download audio"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "28px",
            height: "28px",
            border: "1px solid var(--input-border)",
            borderRadius: "4px",
            backgroundColor: "var(--input-bg)",
            color: "var(--text-color)",
            cursor: hasAudio ? "pointer" : "not-allowed",
            opacity: hasAudio ? 1 : 0.5,
          }}
        >
          <Download size={12} />
        </button>
        <span
          style={{
            fontSize: "12px",
            color: "var(--node-border)",
            fontFamily: "monospace",
          }}
        >
          {hasAudio
            ? `${formatTime(currentTime)} / ${formatTime(duration)}`
            : "No audio"}
        </span>
      </div>
    </div>
  );
};

const DisplayImage = ({ imageUrl }: { imageUrl: string }) => {
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState(false);

  if (error) return null;
  return (
    <div
      className="nodrag"
      style={{
        marginTop: "8px",
        width: "100%",
        flex: "1 1 0",
        minHeight: 0,
        display: loaded ? "flex" : "none",
        justifyContent: "center",
        alignItems: "center",
        backgroundColor: "var(--bg-secondary)",
        borderRadius: "4px",
        overflow: "hidden",
      }}
    >
      <img
        src={imageUrl}
        alt=""
        onLoad={() => setLoaded(true)}
        onError={() => setError(true)}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "contain",
        }}
      />
    </div>
  );
};

export const DisplayImageNode = ({ data }: { data: NodeData }) => {
  const { url: imageUrl } = extractFileData(data, "image", "image");
  if (!imageUrl) return null;
  return <DisplayImage key={imageUrl} imageUrl={imageUrl} />;
};

export const DisplayMarkdownNode = ({ data }: { data: NodeData }) => {
  let markdownContent = data.outputs?.markdown;
  if (!markdownContent) {
    markdownContent = data.markdown;
  }
  if (!markdownContent || typeof markdownContent !== "string") return null;
  return (
    <div
      className="nodrag markdown-body"
      style={{
        marginTop: "8px",
        width: "100%",
        flex: "1 1 0",
        minHeight: "100px",
        overflowY: "auto",
        backgroundColor: "var(--input-bg)",
        border: "1px solid var(--input-border)",
        borderRadius: "4px",
        padding: "16px",
        fontSize: "12px",
        color: "var(--text-color)",
        overflowWrap: "anywhere",
        wordBreak: "normal",
        boxSizing: "border-box",
      }}
    >
      <ReactMarkdown
        components={{
          img: (props) => (
            <img {...props} style={{ maxWidth: "100%", height: "auto" }} />
          ),
          pre: (props) => (
            <pre
              {...props}
              style={{
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
                backgroundColor: "rgba(127, 127, 127, 0.1)",
                padding: "8px",
                borderRadius: "4px",
              }}
            />
          ),
          code: (props) => (
            <code
              {...props}
              style={{
                backgroundColor: "rgba(127, 127, 127, 0.1)",
                padding: "2px 4px",
                borderRadius: "3px",
                fontSize: "85%",
              }}
            />
          ),
        }}
      >
        {markdownContent}
      </ReactMarkdown>
    </div>
  );
};

export const DisplayJsonNode = ({
  data,
  isDark,
}: {
  data: NodeData;
  isDark: boolean;
}) => {
  let jsonContent = data.outputs?.json;
  if (!jsonContent) {
    jsonContent = data.json;
  }
  // parse if it's a string, or use as is if object
  let parsedJson = jsonContent;
  if (typeof jsonContent === "string") {
    try {
      parsedJson = JSON.parse(jsonContent);
    } catch {
      // if parse fails, keep as string or show error?
    }
  }

  if (!parsedJson) return null;

  return (
    <div
      className="nodrag"
      style={{
        margin: "8px 4px 8px 4px",
        width: "auto",
        flex: "1 1 auto",
        minHeight: 0,
        overflow: "auto",
        borderRadius: "4px",
        fontSize: "12px",
        border: "1px solid var(--input-border)",
        backgroundColor: "var(--input-bg)",
        padding: "8px",
        boxSizing: "border-box",
        fontFamily: "monospace",
      }}
    >
      <JsonView
        data={parsedJson}
        shouldExpandNode={allExpanded}
        style={{
          ...(isDark ? darkStyles : defaultStyles),
          container: `${(isDark ? darkStyles : defaultStyles).container} json-view-transparent`,
          stringValue: "json-view-string",
          numberValue: "json-view-number",
          booleanValue: "json-view-boolean",
          nullValue: "json-view-null",
          label: "json-view-label",
          punctuation: "json-view-punctuation",
        }}
      />
    </div>
  );
};
