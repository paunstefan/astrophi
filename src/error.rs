use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AstroPhiError {
    #[error("Astrophi internal error")]
    Internal,
    #[error("GPhoto2 error")]
    GPhoto2(#[from] gphoto2::Error),
    #[error("ParseInt error")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("ParseFloat error")]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("IO error")]
    StdIO(#[from] std::io::Error),
}

impl IntoResponse for AstroPhiError {
    fn into_response(self) -> Response {
        let error_text = match self {
            AstroPhiError::GPhoto2(error) => format!("{:?}", error.kind()),
            AstroPhiError::ParseInt(error) => error.to_string(),
            AstroPhiError::ParseFloat(error) => error.to_string(),
            AstroPhiError::StdIO(error) => format!("{}", error.kind()),
            AstroPhiError::Internal => self.to_string(),
        };
        (StatusCode::INTERNAL_SERVER_ERROR, error_text).into_response()
    }
}
