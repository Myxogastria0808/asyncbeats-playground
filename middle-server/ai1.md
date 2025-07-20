```rust
use std::collections::VecDeque;
use std::sync::Arc;

use crate::{
    errors::{app::AppError, handler::HandlerError},
    models::shared_state::RwLockSharedState,
};
use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::{connect_async, tungstenite};

//* 定数 *//
static SERVER_URL: &str = "ws://localhost:5000";
static WINDOW_SIZE: u64 = 200;
static SLIDE_SIZE: u64 = 100;
const PCM_CHANNEL_CAPACITY: usize = 1000; // 処理が追いつかない場合に備え、1000チャンクまでバッファリング

// WebSocketの書き込み部分を安全に共有するための型エイリアス
type ClientWsSink = Arc<Mutex<SplitSink<WebSocket, Message>>>;

// handler
pub async fn websocket_handler(
    State(_shared_state): State<RwLockSharedState>, // _ をつけて未使用であることを明示
    web_socket: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let response = web_socket.on_upgrade(|socket| async move {
        if let Err(error) = websocket_processing(socket).await {
            tracing::error!("WebSocket processing error: {:?}", error);
        }
        tracing::info!("WebSocket connection closed.");
    });
    Ok(response)
}

// websocket
pub async fn websocket_processing(client_socket: WebSocket) -> Result<(), AppError> {
    // サーバーへ接続
    let (server_ws_stream, _) = connect_async(SERVER_URL)
        .await
        .map_err(HandlerError::TokioTungsteniteError)?;
    tracing::info!("Connection to server established.");

    // ClientとServerのWebSocketを読み書き用に分割
    let (client_writer, client_reader) = client_socket.split();
    let (server_writer, server_reader) = server_ws_stream.split();

    // 複数のタスクからクライアントへの書き込みを安全に行うため、Arc<Mutex<>>でラップ
    let shared_client_writer: ClientWsSink = Arc::new(Mutex::new(client_writer));

    // PCMデータを処理タスクに渡すためのmpscチャンネルを作成
    let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>(PCM_CHANNEL_CAPACITY);

    // --- 3つの独立したタスクを起動 ---

    // [タスク1] ClientからのメッセージをServerへ転送する
    let client_read_task = tokio::spawn(handle_client_messages(client_reader, server_writer));

    // [タスク2] Serverからのメッセージを受信する (Producer)
    let server_read_task = tokio::spawn(handle_server_messages(
        server_reader,
        pcm_tx,
        Arc::clone(&shared_client_writer),
    ));

    // [タスク3] PCMデータを処理してClientへ送信する (Consumer)
    let pcm_processing_task =
        tokio::spawn(process_pcm_data(pcm_rx, Arc::clone(&shared_client_writer)));

    // いずれかのタスクが終了したら、他のタスクも終了させる
    tokio::select! {
        res = client_read_task => tracing::info!("Client read task finished: {:?}", res),
        res = server_read_task => tracing::info!("Server read task finished: {:?}", res),
        res = pcm_processing_task => tracing::info!("PCM processing task finished: {:?}", res),
    }

    Ok(())
}

/// [タスク1] Clientからのメッセージを読み取り、Serverへ転送する
async fn handle_client_messages(
    mut client_reader: SplitStream<WebSocket>,
    mut server_writer: SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Message,
    >,
) {
    while let Some(Ok(msg)) = client_reader.next().await {
        let server_msg = match msg {
            Message::Text(text) => {
                // "open" または "accept" のみを転送
                if text == "open" || text == "accept" {
                    tracing::info!("Forwarding message from client to server: {}", text);
                    tungstenite::Message::Text(text.to_string().into())
                } else {
                    continue;
                }
            }
            Message::Close(_) => {
                tracing::info!("Client sent close frame. Closing connection to server.");
                let _ = server_writer.send(tungstenite::Message::Close(None)).await;
                break;
            }
            _ => continue,
        };

        if server_writer.send(server_msg).await.is_err() {
            tracing::error!("Failed to forward message to server.");
            break;
        }
    }
}

/// [タスク2] Serverからのメッセージを受信し、適切な場所に振り分ける (Producer)
async fn handle_server_messages(
    mut server_reader: SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    pcm_tx: mpsc::Sender<Vec<u8>>,
    client_writer: ClientWsSink,
) {
    while let Some(Ok(msg)) = server_reader.next().await {
        match msg {
            // PCMデータ(Binary)は、処理用のチャンネルへ送信
            tungstenite::Message::Binary(bin) => {
                if pcm_tx.send(bin.to_vec()).await.is_err() {
                    tracing::error!("PCM channel closed. Stopping server message handler.");
                    break;
                }
            }
            // 音声情報(Text)は、直接Clientへ転送
            tungstenite::Message::Text(text) => {
                tracing::info!("Forwarding text from server to client: {}", text);
                let mut writer = client_writer.lock().await;
                if writer
                    .send(Message::Text(text.to_string().into()))
                    .await
                    .is_err()
                {
                    tracing::error!("Failed to send text to client.");
                    break;
                }
            }
            tungstenite::Message::Close(_) => {
                tracing::info!("Server sent close frame. Closing connection to client.");
                let mut writer = client_writer.lock().await;
                let _ = writer.send(Message::Close(None)).await;
                break;
            }
            _ => {}
        }
    }
}

/// [タスク3] チャンネルからPCMデータを受け取り、Sliding Window処理をしてClientへ送信する (Consumer)
async fn process_pcm_data(mut pcm_rx: mpsc::Receiver<Vec<u8>>, client_writer: ClientWsSink) {
    let mut counter: u64 = 0;
    let mut stock_buffer: VecDeque<Vec<u8>> = VecDeque::new();
    let mut send_buffer: Vec<u8> = Vec::new();

    // チャンネルからデータが送られてくるのを待つループ
    while let Some(bin) = pcm_rx.recv().await {
        stock_buffer.push_back(bin);
        counter += 1;

        if counter >= WINDOW_SIZE {
            // スライド幅分だけバッファを準備
            for _ in 0..SLIDE_SIZE {
                if let Some(buf) = stock_buffer.pop_front() {
                    send_buffer.extend(buf);
                }
            }

            // TODO: ここで時間のかかる解析処理を実行する
            // let events = analyze(&send_buffer);
            // let packet = VJDataPacket { pcm_data: send_buffer.clone(), events };
            // let serialized_packet = rmp_serde::to_vec(&packet).unwrap();

            tracing::info!(
                "Sending processed chunk to client. Size: {} bytes",
                send_buffer.len()
            );

            // 処理したデータをクライアントに送信
            let mut writer = client_writer.lock().await;
            if writer
                .send(Message::Binary(send_buffer.clone().into()))
                .await
                .is_err()
            {
                tracing::error!("Failed to send processed PCM to client.");
                break; // 送信に失敗したらループを抜ける
            }

            // カウンタとバッファをリセット
            counter -= SLIDE_SIZE;
            send_buffer.clear();
        }
    }
}
```
