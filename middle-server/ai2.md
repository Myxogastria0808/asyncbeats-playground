```rust
use std::collections::VecDeque;
// use std::sync::Arc; // Arc<Mutex<>> は不要になる

use crate::{
    errors::{app::AppError, handler::HandlerError},
    models::shared_state::RwLockSharedState,
};
use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt}; // stream::{SplitSink, SplitStream} は不要
use tokio::sync::mpsc; // Mutexは不要
use tokio_tungstenite::{connect_async, tungstenite};

//* 定数 *//
static SERVER_URL: &str = "ws://localhost:5000";
static WINDOW_SIZE: u64 = 200;
static SLIDE_SIZE: u64 = 100;
const PROCESSED_DATA_CHANNEL_CAPACITY: usize = 10; // 処理済みデータを10個までバッファリング

// handler
pub async fn websocket_handler(
    State(_shared_state): State<RwLockSharedState>,
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
pub async fn websocket_processing(mut socket: WebSocket) -> Result<(), AppError> {
    // connect to server
    let (mut ws_stream, _) = connect_async(SERVER_URL)
        .await
        .map_err(HandlerError::TokioTungsteniteError)?;
    tracing::info!("Connection to server established.");

    // sliding window 用のバッファ
    let mut stock_buffer: VecDeque<Vec<u8>> = VecDeque::new();
    let mut counter: u64 = 0;

    // --- 変更点 ---
    // 処理が完了したデータをメインループに返すためのチャンネル
    let (processed_tx, mut processed_rx) =
        mpsc::channel::<Vec<u8>>(PROCESSED_DATA_CHANNEL_CAPACITY);

    loop {
        tokio::select! {
            // [分岐1] Clientからのメッセージ受信
            client_msg = socket.recv() => {
                let msg = match client_msg {
                    Some(Ok(msg)) => msg,
                    // クライアントが切断またはエラー
                    _ => {
                        tracing::info!("Client disconnected.");
                        let _ = ws_stream.send(tungstenite::Message::Close(None)).await;
                        break;
                    }
                };

                // clientからのメッセージをserverへ転送
                if let Message::Text(text) = msg {
                    if text == "open" || text == "accept" {
                        let _ = ws_stream.send(tungstenite::Message::Text(text.to_string().into())).await;
                    }
                } else if let Message::Close(_) = msg {
                    tracing::info!("Client sent close frame.");
                    let _ = ws_stream.send(tungstenite::Message::Close(None)).await;
                    break;
                }
            },

            // [分岐2] Serverからのメッセージ受信
            server_msg = ws_stream.next() => {
                 let msg = match server_msg {
                    Some(Ok(msg)) => msg,
                     // サーバーが切断またはエラー
                    _ => {
                        tracing::info!("Server disconnected.");
                        let _ = socket.close().await;
                        break;
                    }
                };

                match msg {
                    // 音声情報(Text)はそのままクライアントへ転送
                    tungstenite::Message::Text(text) => {
                        if socket.send(Message::Text(text.to_string().into())).await.is_err() {
                            break; // 送信失敗なら終了
                        }
                    }
                    // PCMデータ(Binary)を受信した場合
                    tungstenite::Message::Binary(bin) => {
                        stock_buffer.push_back(bin.to_vec());
                        counter += 1;

                        // Sliding Windowの条件を満たしたら、処理を別タスクに切り出す
                        if counter >= WINDOW_SIZE {
                            let mut send_buffer: Vec<u8> = Vec::new();
                            for _ in 0..SLIDE_SIZE {
                                if let Some(buf) = stock_buffer.pop_front() {
                                    send_buffer.extend(buf);
                                }
                            }
                            counter -= SLIDE_SIZE;

                            // --- 変更点 ---
                            // 処理タスクをspawn。txをcloneしてタスクに渡す
                            let tx = processed_tx.clone();
                            tokio::spawn(async move {
                                // TODO: ここで時間のかかる解析処理を実行
                                // let events = analyze(&send_buffer); ...

                                // 処理結果をチャンネル経由でメインループに送信
                                if tx.send(send_buffer).await.is_err() {
                                    tracing::error!("Failed to send processed data to main loop. Receiver closed.");
                                }
                            });
                        }
                    }
                    tungstenite::Message::Close(_) => {
                        tracing::info!("Server sent close frame.");
                        let _ = socket.close().await;
                        break;
                    }
                    _ => {}
                }
            },

            // --- 変更点 ---
            // [分岐3] 処理済みデータをチャンネルから受信し、Clientへ送信する
            Some(processed_data) = processed_rx.recv() => {
                tracing::info!(
                    "Sending processed chunk to client. Size: {} bytes",
                    processed_data.len()
                );
                if socket.send(Message::Binary(processed_data.into())).await.is_err() {
                    // 送信に失敗したらループを抜けて接続を閉じる
                    break;
                }
            }
        }
    }
    Ok(())
}
```