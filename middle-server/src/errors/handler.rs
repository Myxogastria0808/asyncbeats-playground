use super::app::AppError;
use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("UnexpectedMessageTypeError: unsupported message type received")]
    UnexpectedMessageTypeError,
    #[error("UnexpectedMessageError: {0}")]
    UnexpectedMessageError(String),
    #[error(transparent)]
    SetGlobalDefaultError(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    AxumError(#[from] axum::Error),
    #[error(transparent)]
    TokioTungsteniteError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("AudioInfoError: {0}")]
    AudioInfoError(String),
}

impl From<HandlerError> for AppError {
    fn from(error: HandlerError) -> Self {
        match error {
            HandlerError::UnexpectedMessageTypeError => AppError {
                status_code: StatusCode::BAD_REQUEST,
                message: "UnexpectedMessageTypeError: unsupported message type received".into(),
            },
            HandlerError::UnexpectedMessageError(e) => AppError {
                status_code: StatusCode::BAD_REQUEST,
                message: format!("UnexpectedMessageError: {e}"),
            },
            HandlerError::SetGlobalDefaultError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("SetGlobalDefaultError: {e}"),
            },
            HandlerError::IoError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("IoError: {e}"),
            },
            HandlerError::AxumError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("AxumError: {e}"),
            },
            HandlerError::TokioTungsteniteError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("TokioTungsteniteError: {e}"),
            },
            HandlerError::ParseIntError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("ParseIntError: {e}"),
            },
            HandlerError::AudioInfoError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("AudioInfoError: {e}"),
            },
        }
    }
}
