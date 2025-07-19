use std::collections::VecDeque;

use crate::{
    errors::{app::AppError, handler::HandlerError},
    models::shared_state::RwLockSharedState,
};
use axum::extract::ws::WebSocket;
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{TryStreamExt, sink::SinkExt};
use tokio_tungstenite::connect_async;

//* Constant values *//
static SERVER_URL: &str = "ws://localhost:5000";
static WINDOW_SIZE: u64 = 200;
static SLIDE_SIZE: u64 = 100;

// handler
pub async fn websocket_handler(
    State(shared_state): State<RwLockSharedState>,
    web_socket: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let shared_state = shared_state.read().await;
    let response = web_socket.on_upgrade(|socket| async move {
        if let Err(error) = websocket_processing(socket).await {
            tracing::error!("WebSocket error: {:?}", error);
        }
    });
    drop(shared_state);
    Ok(response)
}

// websocket
pub async fn websocket_processing(mut socket: WebSocket) -> Result<(), AppError> {
    // sliding window
    let mut counter: u64 = 0;
    let mut stock_buffer: VecDeque<Vec<u8>> = VecDeque::new();
    let mut send_buffer: Vec<u8> = Vec::new();

    // connect to server
    let (mut ws_stream, _) = connect_async(SERVER_URL)
        .await
        .map_err(HandlerError::TokioTungsteniteError)?;
    tracing::info!("webSocket connection established");

    loop {
        tokio::select! {
            // Handle Client messages
            client_msg = socket.recv() => {
                // client -> middle-server
                match client_msg {
                    Some(Ok(client_msg)) => {
                        match client_msg {
                            axum::extract::ws::Message::Text(text) => {
                                // receive connection request from client
                                let msg = text.to_string();
                                if msg != "open" && msg != "accept" {
                                    tracing::info!("Received unexpected text: {:?}", msg);
                                    return Err(HandlerError::UnexpectedMessageError(msg).into());
                                }
                                tracing::info!("Received text: {:?}", msg);
                                //* step1: receive open message from client and send to server *//
                                if msg == "open" {
                                    // send open message to server
                                    ws_stream.send(tokio_tungstenite::tungstenite::Message::Text("open".into())).await
                                        .map_err(HandlerError::TokioTungsteniteError)?;
                                }
                                //* step4: receive accept message from client and send to server *//
                                if msg == "accept" {
                                    // send accept message to server
                                    ws_stream.send("accept".into()).await
                                        .map_err(HandlerError::TokioTungsteniteError)?;
                                }
                            }
                            axum::extract::ws::Message::Close(close) => {
                                tracing::info!("Client disconnected: {:?}", close);

                                //* send close frame to server *//
                                // convert axum CloseFrame to tungstenite CloseFrame
                                let tungstenite_close = close.map(|close_frame| {
                                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                        code: tungstenite::protocol::frame::coding::CloseCode::from(close_frame.code),
                                        reason: close_frame.reason.to_string().into(),
                                    }
                                });
                                // send close frame to server
                                ws_stream.send(tokio_tungstenite::tungstenite::Message::Close(tungstenite_close)).await
                                    .map_err(HandlerError::TokioTungsteniteError)?;

                                break;
                            }
                            _ => {
                                tracing::error!("Received unsupported message type from client");
                                return Err(HandlerError::UnexpectedMessageTypeError.into());
                            }
                        }
                    },
                    Some(Err(e)) => {
                        tracing::error!("Received client error: {:?}", e);
                        return Err(HandlerError::AxumError(e).into());
                    },
                    None => {
                        tracing::info!("Client closed the connection");

                        //* send close frame to server *//
                        ws_stream.send(tokio_tungstenite::tungstenite::Message::Close(None)).await
                            .map_err(HandlerError::TokioTungsteniteError)?;

                        break;
                    },
                };
            }
            // server -> middle-server
            server_msg = ws_stream.try_next() => {
                match server_msg {
                    Ok(Some(server_msg)) => {
                        match server_msg {
                            tokio_tungstenite::tungstenite::Message::Text(text) => {
                                tracing::info!("Received text: {:?}", text);
                                //* step2: receive audio info from server *//
                                //TODO 以下のパース処理をまとめる
                                let audio_info_vec: Vec<u64> = String::from_utf8_lossy(text.as_bytes())
                                    .split(' ')
                                    .map(|s| s.parse::<u64>().map_err(HandlerError::ParseIntError))
                                    .collect::<Result<Vec<u64>, HandlerError>>()?;
                                // validate audio info
                                if audio_info_vec.len() != 2 {
                                    return Err(HandlerError::AudioInfoError(text.to_string()).into());
                                }
                                let channel = match audio_info_vec.first().cloned() {
                                    Some(channel) => channel,
                                    None => return Err(HandlerError::AudioInfoError(text.to_string()).into()),
                                };
                                let sample_rate = match audio_info_vec.get(1).cloned() {
                                    Some(sample_rate) => sample_rate,
                                    None => return Err(HandlerError::AudioInfoError(text.to_string()).into()),
                                };

                                //* step3: send audio info to client *//
                                socket.send(axum::extract::ws::Message::Text(format!("{channel} {sample_rate}").into())).await.map_err(HandlerError::AxumError)?;
                            }
                            tokio_tungstenite::tungstenite::Message::Binary(bin) => {
                                tracing::info!("counter: {}", counter);
                                //* step5: receive binary and send it to client *//

                                //* step6: do sliding window *//
                                if counter > WINDOW_SIZE {
                                    //* send buffer *//
                                    for _ in 0..SLIDE_SIZE {
                                        if let Some(buf) = stock_buffer.pop_front() {
                                            send_buffer.extend(buf);
                                        }
                                    }
                                    //* step7: send binary data to client with window size *//
                                    socket.send(axum::extract::ws::Message::Binary(send_buffer.clone().into())).await.map_err(HandlerError::AxumError)?;

                                    // reset counter
                                    counter -= SLIDE_SIZE;
                                    // reset send buffer
                                    send_buffer.clear();
                                } else {
                                    //* collect buffer *//
                                    counter += 1;
                                    stock_buffer.push_back(bin.to_vec());
                                }

                            }

                            tokio_tungstenite::tungstenite::Message::Close(close) => {
                                tracing::info!("Server disconnected: {:?}", close);

                                //* send close frame to client *//
                                // convert tungstenite CloseFrame to axum CloseFrame
                                let axum_close = close.map(|close_frame| axum::extract::ws::CloseFrame {
                                    code: axum::extract::ws::CloseCode::from(u16::from(close_frame.code)),
                                    reason: close_frame.reason.to_string().into(),
                                });
                                // send close frame to client
                                socket.send(axum::extract::ws::Message::Close(axum_close)).await.map_err(HandlerError::AxumError)?;

                                break;
                            }
                            _ => {
                                tracing::error!("Received unsupported message type from server");
                                return Err(HandlerError::UnexpectedMessageTypeError.into());
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::info!("Server closed the connection");

                        //* send close frame to client *//
                        socket.send(axum::extract::ws::Message::Close(None)).await.map_err(HandlerError::AxumError)?;

                        break;
                    }
                    Err(e) => {
                        tracing::error!("Server receive error: {:?}", e);
                        return Err(HandlerError::TokioTungsteniteError(e).into());
                    }
                }
            }
        }
    }
    Ok(())
}
