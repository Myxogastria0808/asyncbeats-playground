import React, { useState, useRef, useEffect, useCallback } from "react";

// Windowインターフェースを拡張して、webkitAudioContextの型定義を追加
// これにより、(window as any) を使わずに型安全なアクセスが可能になります。
declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}

// スタイルを定義
const styles: { [key: string]: React.CSSProperties } = {
  container: {
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
    maxWidth: "800px",
    margin: "40px auto",
    padding: "20px",
    backgroundColor: "#f7f9fc",
    borderRadius: "12px",
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.1)",
    color: "#333",
  },
  title: {
    color: "#1a237e",
    textAlign: "center",
    marginBottom: "30px",
  },
  controlPanel: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    marginBottom: "20px",
    flexWrap: "wrap",
  },
  input: {
    flex: 1,
    padding: "12px",
    border: "1px solid #ccc",
    borderRadius: "8px",
    fontSize: "16px",
    minWidth: "250px",
  },
  button: {
    padding: "12px 24px",
    border: "none",
    borderRadius: "8px",
    fontSize: "16px",
    fontWeight: "bold",
    cursor: "pointer",
    transition: "background-color 0.3s, transform 0.1s",
  },
  connectButton: {
    backgroundColor: "#4caf50",
    color: "white",
  },
  disconnectButton: {
    backgroundColor: "#f44336",
    color: "white",
  },
  statusPanel: {
    backgroundColor: "white",
    padding: "20px",
    borderRadius: "8px",
    border: "1px solid #e0e0e0",
  },
  statusGrid: {
    display: "grid",
    gridTemplateColumns: "150px 1fr",
    gap: "12px",
    alignItems: "center",
  },
  statusLabel: {
    fontWeight: "bold",
    color: "#555",
  },
  statusValue: {
    wordBreak: "break-all",
    backgroundColor: "#eee",
    padding: "6px 10px",
    borderRadius: "6px",
  },
};

/**
 * @type AudioInfo
 * @description サーバーから受信するオーディオ情報の型定義
 * @property {number} channel - チャンネル数 (e.g., 1 for mono, 2 for stereo)
 * @property {number} sample_rate - サンプルレート (e.g., 44100)
 */
type AudioInfo = {
  channel: number;
  sample_rate: number;
};

// 再生を開始するために必要なバッファの数を定義
const PLAYBACK_BUFFER_THRESHOLD = 5;

const App: React.FC = () => {
  // --- State Hooks ---
  const [url, setUrl] = useState<string>("ws://localhost:7001");
  const [isConnected, setIsConnected] = useState<boolean>(false);
  const [statusMessage, setStatusMessage] = useState<string>("未接続");
  const [audioInfo, setAudioInfo] = useState<AudioInfo | null>(null);
  const [bufferSize, setBufferSize] = useState<number>(0);

  // --- Ref Hooks ---
  const webSocketRef = useRef<WebSocket | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const pcmBufferRef = useRef<Float32Array[]>([]);
  const isPlayingRef = useRef<boolean>(false);
  const audioInfoRef = useRef<AudioInfo | null>(null);

  /**
   * @function handleDisconnect
   * @description WebSocket接続を切断し、関連する状態をすべて初期化する
   */
  const handleDisconnect = useCallback(() => {
    if (webSocketRef.current) {
      webSocketRef.current.close();
      webSocketRef.current = null;
    }
    if (audioContextRef.current) {
      audioContextRef.current.close().catch(console.error);
      audioContextRef.current = null;
    }
    setIsConnected(false);
    setAudioInfo(null);
    audioInfoRef.current = null;
    setStatusMessage("切断されました");
    setBufferSize(0);
    pcmBufferRef.current = [];
    isPlayingRef.current = false;
  }, []);

  /**
   * @function playNextChunk
   * @description pcmBufferから次のオーディオチャンクを再生する。
   */
  const playNextChunk = useCallback(() => {
    if (!isPlayingRef.current || pcmBufferRef.current.length === 0) {
      isPlayingRef.current = false;
      setStatusMessage("バッファが空です。データ受信待機中...");
      return;
    }

    const audioContext = audioContextRef.current;
    const currentAudioInfo = audioInfoRef.current;

    if (!audioContext || !currentAudioInfo) {
      console.error(
        "再生を試みましたが、AudioContextまたはAudioInfoがありません。"
      );
      isPlayingRef.current = false;
      return;
    }

    const pcmData = pcmBufferRef.current.shift()!;
    setBufferSize(pcmBufferRef.current.length);

    const frameCount = pcmData.length / currentAudioInfo.channel;
    const audioBuffer = audioContext.createBuffer(
      currentAudioInfo.channel,
      frameCount,
      currentAudioInfo.sample_rate
    );

    if (currentAudioInfo.channel === 1) {
      audioBuffer.copyToChannel(pcmData, 0);
    } else {
      for (let ch = 0; ch < currentAudioInfo.channel; ch++) {
        const channelData = new Float32Array(frameCount);
        for (let i = 0; i < frameCount; i++) {
          channelData[i] = pcmData[i * currentAudioInfo.channel + ch];
        }
        audioBuffer.copyToChannel(channelData, ch);
      }
    }

    const source = audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(audioContext.destination);
    source.onended = playNextChunk;
    source.start();

    setStatusMessage(
      `チャンクを再生中... 残りバッファ: ${pcmBufferRef.current.length}`
    );
  }, []);

  /**
   * @function startPlayback
   * @description Web Audio APIを利用してPCMデータの再生を開始する
   */
  const startPlayback = useCallback(() => {
    const currentAudioInfo = audioInfoRef.current;
    if (isPlayingRef.current || !currentAudioInfo) return;

    if (!audioContextRef.current) {
      // ★★★★★ 修正点 ★★★★★
      // (window as any) を使わずに、型安全な方法でwebkitAudioContextにアクセス
      const AudioCtor = window.AudioContext || window.webkitAudioContext;
      // ★★★★★★★★★★★★★★★★
      if (!AudioCtor) {
        setStatusMessage(
          "エラー: このブラウザはWeb Audio APIをサポートしていません。"
        );
        return;
      }
      audioContextRef.current = new AudioCtor({
        sampleRate: currentAudioInfo.sample_rate,
      });
    }

    if (audioContextRef.current.state === "suspended") {
      audioContextRef.current.resume();
    }

    isPlayingRef.current = true;
    setStatusMessage("再生を開始します");
    playNextChunk();
  }, [playNextChunk]);

  /**
   * @function handleConnect
   * @description WebSocketで接続し、イベントハンドラを設定する
   */
  const handleConnect = () => {
    if (webSocketRef.current) return;

    try {
      const ws = new WebSocket(url);
      ws.binaryType = "arraybuffer";
      webSocketRef.current = ws;
      setStatusMessage(`接続試行中: ${url}`);

      ws.onopen = () => {
        setIsConnected(true);
        setStatusMessage('接続成功。 "open" を送信します...');
        ws.send("open");
      };

      ws.onmessage = (event: MessageEvent) => {
        if (typeof event.data === "string") {
          setStatusMessage(`サーバーからメッセージ受信: ${event.data}`);
          const parts = event.data.split(" ");
          if (parts.length === 2) {
            const channel = parseInt(parts[0], 10);
            const sample_rate = parseInt(parts[1], 10);

            if (!isNaN(channel) && !isNaN(sample_rate)) {
              const newAudioInfo = { channel, sample_rate };
              setAudioInfo(newAudioInfo);
              audioInfoRef.current = newAudioInfo;
              setStatusMessage('AudioInfo 受信完了。"accept" を送信します。');
              ws.send("accept");
            } else {
              setStatusMessage(
                `エラー: 不正なAudioInfo形式です: ${event.data}`
              );
            }
          }
        } else if (event.data instanceof ArrayBuffer) {
          if (!audioInfoRef.current) {
            console.error("Received PCM data before AudioInfo was processed.");
            setStatusMessage(
              "エラー: AudioInfoの処理より先にPCMデータを受信しました"
            );
            return;
          }

          const pcmDataInt16 = new Int16Array(event.data);
          const pcmDataFloat32 = new Float32Array(pcmDataInt16.length);
          for (let i = 0; i < pcmDataInt16.length; i++) {
            pcmDataFloat32[i] = pcmDataInt16[i] / 32768.0;
          }

          pcmBufferRef.current.push(pcmDataFloat32);
          const currentBufferSize = pcmBufferRef.current.length;
          setBufferSize(currentBufferSize);
          setStatusMessage(
            `PCMデータ受信。バッファサイズ: ${currentBufferSize}`
          );

          if (
            currentBufferSize >= PLAYBACK_BUFFER_THRESHOLD &&
            !isPlayingRef.current
          ) {
            startPlayback();
          }
        }
      };

      ws.onclose = () => {
        console.log("WebSocket connection closed.");
        handleDisconnect();
      };

      ws.onerror = (error) => {
        console.error("WebSocket error:", error);
        setStatusMessage(`WebSocketエラーが発生しました。`);
        handleDisconnect();
      };
    } catch (error) {
      console.error("Failed to create WebSocket:", error);
      setStatusMessage(
        `接続に失敗しました: ${error instanceof Error ? error.message : "Unknown error"
        }`
      );
    }
  };

  useEffect(() => {
    return () => {
      handleDisconnect();
    };
  }, [handleDisconnect]);

  return (
    <div style={styles.container}>
      <h1 style={styles.title}>React WebSocket Audio Client</h1>

      <div style={styles.controlPanel}>
        <input
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={isConnected}
          style={styles.input}
          placeholder="例: ws://localhost:8080"
        />
        <button
          onClick={isConnected ? handleDisconnect : handleConnect}
          style={{
            ...styles.button,
            ...(isConnected ? styles.disconnectButton : styles.connectButton),
          }}
        >
          {isConnected ? "切断" : "接続"}
        </button>
      </div>

      <div style={styles.statusPanel}>
        <div style={styles.statusGrid}>
          <span style={styles.statusLabel}>システムメッセージ:</span>
          <span style={styles.statusValue}>{statusMessage}</span>

          <span style={styles.statusLabel}>接続状態:</span>
          <span style={styles.statusValue}>
            {isConnected ? "オンライン" : "オフライン"}
          </span>

          <span style={styles.statusLabel}>オーディオ情報:</span>
          <span style={styles.statusValue}>
            {audioInfo
              ? `${audioInfo.channel}ch @ ${audioInfo.sample_rate}Hz`
              : "N/A"}
          </span>

          <span style={styles.statusLabel}>再生バッファサイズ:</span>
          <span style={styles.statusValue}>{bufferSize}</span>
        </div>
      </div>
    </div>
  );
};

export default App;
