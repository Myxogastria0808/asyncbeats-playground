import { useState, useRef, useCallback } from "react";
import "./App.css";

const MIDDLE_SERVER_URL = "ws://localhost:7000";
// 再生を開始するために最低限必要なバッファ内のチャンク数
const BUFFER_PLAYBACK_THRESHOLD = 5;

function App() {
  const [isConnected, setIsConnected] = useState(false);
  const [status, setStatus] = useState("Disconnected");
  const [audioInfo, setAudioInfo] = useState<{
    channels: number;
    sampleRate: number;
  } | null>(null);

  // --- useRefで管理する変数群 ---
  const ws = useRef<WebSocket | null>(null);
  const audioContext = useRef<AudioContext | null>(null);
  // 受信したPCMチャンクを溜めておくキュー (バッファ)
  const bufferQueue = useRef<ArrayBuffer[]>([]);
  // 次の音声チャンクを再生する開始時間
  const nextStartTime = useRef<number>(0);
  // 再生処理のインターバルID
  const playerIntervalId = useRef<number | null>(null);

  /**
   * 再生ループのメイン処理。定期的に呼び出される。
   */
  const playerTick = useCallback(() => {
    if (!audioContext.current || !audioInfo) return;

    // 前のチャンクの再生が終わり、かつバッファに次のデータがある場合に再生処理を行う
    const safetyMargin = 0.1; // 100msの安全マージン
    if (
      audioContext.current.currentTime > nextStartTime.current - safetyMargin &&
      bufferQueue.current.length > 0
    ) {
      const arrayBuffer = bufferQueue.current.shift()!; // バッファからチャンクを1つ取り出す

      // --- デコード処理 ---
      const pcmData = new Int16Array(arrayBuffer);
      const frameCount = pcmData.length / audioInfo.channels;
      if (frameCount <= 0) return;

      const audioBuffer = audioContext.current.createBuffer(
        audioInfo.channels,
        frameCount,
        audioContext.current.sampleRate
      );

      // デインターリーブ
      for (let ch = 0; ch < audioInfo.channels; ch++) {
        const channelData = audioBuffer.getChannelData(ch);
        for (let i = 0; i < frameCount; i++) {
          channelData[i] = pcmData[i * audioInfo.channels + ch] / 32768.0;
        }
      }

      // --- 再生スケジューリング ---
      const source = audioContext.current.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.current.destination);

      const currentTime = audioContext.current.currentTime;
      // nextStartTimeが過去の時刻なら、現在時刻を基準に再設定
      const startTime =
        nextStartTime.current > currentTime
          ? nextStartTime.current
          : currentTime;

      source.start(startTime);
      console.log(
        `[Player] Scheduled chunk to play at ${startTime.toFixed(
          2
        )}s. Buffer size: ${bufferQueue.current.length}`
      );

      // 次のチャンクの開始時刻を更新
      nextStartTime.current = startTime + audioBuffer.duration;
    }
  }, [audioInfo]); // audioInfoがセットされたらこの関数が再生成される

  /**
   * 再生ループを開始する
   */
  const startPlayer = useCallback(() => {
    if (playerIntervalId.current !== null) return; // すでに開始されている場合は何もしない
    console.log("[Player] Starting player loop.");
    // 50msごとにplayerTickを実行
    playerIntervalId.current = window.setInterval(playerTick, 50);
  }, [playerTick]);

  /**
   * 再生ループを停止する
   */
  const stopPlayer = () => {
    if (playerIntervalId.current === null) return;
    console.log("[Player] Stopping player loop.");
    window.clearInterval(playerIntervalId.current);
    playerIntervalId.current = null;
  };

  /**
   * サーバーへの接続を開始
   */
  const handleConnect = () => {
    // 既存の接続やバッファをリセット
    ws.current?.close();
    bufferQueue.current = [];
    nextStartTime.current = 0;

    if (!audioContext.current) {
      audioContext.current = new AudioContext();
    }
    if (audioContext.current.state === "suspended") {
      audioContext.current.resume();
    }

    setStatus("Connecting...");
    ws.current = new WebSocket(MIDDLE_SERVER_URL);

    ws.current.onopen = () => {
      setStatus('Connection established. Sending "open"...');
      ws.current?.send("open");
    };

    ws.current.onmessage = async (event: MessageEvent) => {
      // 文字列メッセージ (AudioInfo) の処理
      if (typeof event.data === "string") {
        setStatus('Audio info received. Sending "accept"...');
        const parts = event.data.split(" ");
        const channels = parseInt(parts[0], 10);
        const sampleRate = parseInt(parts[1], 10);

        if (audioContext.current?.sampleRate !== sampleRate) {
          console.warn(
            `Browser AudioContext SR (${audioContext.current?.sampleRate}) is different from server SR (${sampleRate}). Playback quality may be affected.`
          );
        }

        setAudioInfo({ channels, sampleRate });
        ws.current?.send("accept");
        setIsConnected(true);
        setStatus("Buffering initial data...");
      }
      // バイナリメッセージ (PCMデータ) の処理
      else if (event.data instanceof Blob) {
        const arrayBuffer = await event.data.arrayBuffer();
        // 受信したデータをバッファに追加
        bufferQueue.current.push(arrayBuffer);

        // バッファが閾値に達したら再生ループを開始
        if (
          bufferQueue.current.length >= BUFFER_PLAYBACK_THRESHOLD &&
          playerIntervalId.current === null
        ) {
          setStatus("Buffer filled. Starting playback...");
          startPlayer();
        }
      }
    };

    ws.current.onclose = () => {
      setStatus("Disconnected");
      setIsConnected(false);
      setAudioInfo(null);
      stopPlayer();
    };

    ws.current.onerror = (error) => {
      console.error("WebSocket error:", error);
      setStatus("Connection Error");
      setIsConnected(false);
      stopPlayer();
    };
  };

  const handleDisconnect = () => {
    ws.current?.close();
  };

  return (
    <div className="card">
      <h1>Asynchronous VJ Client</h1>
      <div>
        {!isConnected ? (
          <button onClick={handleConnect}>Connect & Play</button>
        ) : (
          <button onClick={handleDisconnect} disabled={!isConnected}>
            Disconnect
          </button>
        )}
      </div>
      <div className="status">
        <p>{status}</p>
        {isConnected && <p>Buffer size: {bufferQueue.current.length}</p>}
      </div>
      {audioInfo && (
        <div>
          <p>
            Channels: <strong>{audioInfo.channels}</strong> | Sample Rate:{" "}
            <strong>{audioInfo.sampleRate}</strong> Hz
          </p>
        </div>
      )}
    </div>
  );
}

export default App;
