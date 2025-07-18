use super::app::AppError;
use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

impl From<AnalyzerError> for AppError {
    fn from(error: AnalyzerError) -> Self {
        match error {
            AnalyzerError::IoError(e) => AppError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("IoError: {e}"),
            },
        }
    }
}
