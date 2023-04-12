use axum::{http::StatusCode, response::Html, routing::get, Json, Router};
use error::AstroPhiError;
use gphoto2::{widget::RadioWidget, Context};
use serde::{Deserialize, Serialize};
use std::{fs, net::SocketAddr};

mod error;

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let app = Router::new()
        .route("/", get(root))
        .route("/info", get(camera_info));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Handler for the main page
async fn root() -> Result<Html<String>, StatusCode> {
    let html = Html(fs::read_to_string("static/index.html").map_err(|_| StatusCode::NOT_FOUND)?);
    Ok(html)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CameraInfo {
    pub iso: u32,
    pub aperture: f32,
    pub exposure: f32,
    pub capturetarget: String,
}

async fn camera_info() -> Result<Json<CameraInfo>, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    let iso: u32 = camera
        .config_key::<RadioWidget>("iso")
        .wait()?
        .choice()
        .parse()?;
    tracing::debug!("Parsed ISO: {:?}", iso);

    let exposure: f32 = camera
        .config_key::<RadioWidget>("shutterspeed")
        .wait()?
        .choice()
        .parse()?;
    tracing::debug!("Parsed shutterspeed: {:?}", exposure);

    let aperture: f32 = camera
        .config_key::<RadioWidget>("aperture")
        .wait()?
        .choice()
        .parse()?;
    tracing::debug!("Parsed aperture: {:?}", aperture);

    // "Internal RAM" = 0
    // "Memory card"  = 1
    let capturetarget = camera
        .config_key::<RadioWidget>("capturetarget")
        .wait()?
        .choice();
    tracing::debug!("Parsed capturetarget: {:?}", capturetarget);

    let info = CameraInfo {
        iso,
        aperture,
        exposure,
        capturetarget,
    };

    Ok(Json(info))
}
