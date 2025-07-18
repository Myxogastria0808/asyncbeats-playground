use crate::{
    errors::{app::AppError, handler::HandlerError},
    models::shared_state::RwLockSharedState,
};
use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};

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

//websocket
pub async fn websocket_processing(mut socket: WebSocket) -> Result<(), AppError> {
    while let Some(message) = socket.recv().await {
        // Receive a message from the client
        match message {
            Ok(message) => {
                match message {
                    Message::Text(text) => {
                        // receive connection request from client
                        let msg = text.to_string();
                        if msg != "open" {
                            tracing::info!("Received unexpected text: {:?}", msg);
                            return Err(HandlerError::UnexpectedMessageError(msg).into());
                        }
                        tracing::info!("Received text: {:?}", msg);
                    }
                    Message::Binary(binary) => {
                        tracing::error!("Received binary: {:?}", binary);
                        return Err(HandlerError::UnexpectedMessageTypeError.into());
                    }
                    Message::Close(close) => {
                        tracing::info!("Client disconnected: {:?}", close);
                        return Ok(());
                    }
                    _ => {
                        tracing::error!("Received unsupported message type from server");
                        return Err(HandlerError::UnexpectedMessageTypeError.into());
                    }
                }
            }
            Err(error) => {
                tracing::error!("Error receiving message: {}", error);
                return Err(HandlerError::from(error).into());
            }
        }
    }
    Ok(())
}
