const WAVEFORM_SAMPLES = 100;

/**
 * Generates waveform data from an audio data URL or URL
 */
export async function generateWaveformData(url: string): Promise<number[]> {
  try {
    const response = await fetch(url);
    const arrayBuffer = await response.arrayBuffer();
    const audioContext = new AudioContext();
    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

    const rawData = audioBuffer.getChannelData(0);
    const blockSize = Math.floor(rawData.length / WAVEFORM_SAMPLES);
    const filteredData: number[] = [];

    for (let i = 0; i < WAVEFORM_SAMPLES; i++) {
      let sum = 0;
      for (let j = 0; j < blockSize; j++) {
        sum += Math.abs(rawData[i * blockSize + j]);
      }
      filteredData.push(sum / blockSize);
    }

    const maxVal = Math.max(...filteredData);
    const normalized =
      maxVal > 0 ? filteredData.map((v) => v / maxVal) : filteredData;
    audioContext.close();
    return normalized;
  } catch (e) {
    console.error("Error generating waveform:", e);
    return [];
  }
}

// re-export the constant for use in AudioWaveform
export { WAVEFORM_SAMPLES };
