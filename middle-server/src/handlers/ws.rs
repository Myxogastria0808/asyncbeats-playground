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
static WINDOW_SIZE: usize = 1000;
static SLIDE_SIZE: usize = 500;

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
                                if msg != "open" {
                                    tracing::info!("Received unexpected text: {:?}", msg);
                                    return Err(HandlerError::UnexpectedMessageError(msg).into());
                                }
                                tracing::info!("Received text: {:?}", msg);
                                // send connection request to server
                                // This program uses tokio-tungstenite to send message
                                ws_stream.send("open".into()).await
                                    .map_err(HandlerError::TokioTungsteniteError)?;
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
                        tracing::info!("Received message from server: {:?}", server_msg);

                        match server_msg {
                            tokio_tungstenite::tungstenite::Message::Text(text) => {
                                // こりあえずそのままテキストをclientに転送する
                                let audio_info = text.to_string();
                                socket.send(axum::extract::ws::Message::Text(audio_info.into())).await.map_err(HandlerError::AxumError)?;
                                //TODO: test
                                // server に acceptを送信する
                                ws_stream.send("accept".into()).await
                                    .map_err(HandlerError::TokioTungsteniteError)?;
                            }
                            tokio_tungstenite::tungstenite::Message::Binary(bin) => {
                                // こりあえずそのままPCMデータをclientに転送する
                                socket.send(axum::extract::ws::Message::Binary(bin)).await.map_err(HandlerError::AxumError)?;
                            }
                            tokio_tungstenite::tungstenite::Message::Close(close) => {
                                tracing::info!("Server disconnected: {:?}", close);

                                //* send close frame to client *//
                                // convert tungstenite CloseFrame to axum CloseFrame
                                let axum_close = close.map(|close_frame| axum::extract::ws::CloseFrame {
                                    code: axum::extract::ws::CloseCode::from(u16::from(close_frame.code)),
                                    reason: close_frame.reason.to_string().into(),
                                });
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
