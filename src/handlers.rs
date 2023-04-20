use crate::{
    error::AstroPhiError, CameraInfo, Command, Config, LOG_DIR, LOG_FILE, TEMP_FILE, TOTAL_FRAMES,
};
use axum::{http::StatusCode, response::Html, Json};
use gphoto2::{widget::RadioWidget, Context};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use std::{thread, time};

// Handler for the main page
pub async fn root() -> Result<Html<String>, StatusCode> {
    tracing::info!("GET root");

    let html = Html(fs::read_to_string("static/index.html").map_err(|_| StatusCode::NOT_FOUND)?);
    Ok(html)
}

pub async fn camera_info() -> Result<Json<CameraInfo>, AstroPhiError> {
    tracing::info!("GET /info");

    let camera = Context::new()?.autodetect_camera().wait()?;

    let iso: u32 = camera
        .config_key::<RadioWidget>("iso")
        .wait()?
        .choice()
        .parse()?;
    tracing::info!("Parsed ISO: {:?}", iso);

    let exposure: f32 = parse_shutterspeed(
        &camera
            .config_key::<RadioWidget>("shutterspeed")
            .wait()?
            .choice(),
    )?;
    tracing::info!("Parsed shutterspeed: {:?}", exposure);

    let aperture: f32 = camera
        .config_key::<RadioWidget>("aperture")
        .wait()?
        .choice()
        .parse()?;
    tracing::info!("Parsed aperture: {:?}", aperture);

    // "Internal RAM" = 0
    // "Memory card"  = 1
    let capturetarget = camera
        .config_key::<RadioWidget>("capturetarget")
        .wait()?
        .choice();
    tracing::info!("Parsed capturetarget: {:?}", capturetarget);

    let total_frames = TOTAL_FRAMES.lock().unwrap();

    let info = CameraInfo {
        iso,
        aperture,
        exposure,
        capturetarget,
        total_frames: *total_frames,
    };

    Ok(Json(info))
}

fn parse_shutterspeed(shutterspeed: &str) -> Result<f32, AstroPhiError> {
    if shutterspeed.contains('/') {
        shutterspeed
            .split('/')
            .map(|s| s.trim().parse::<f32>().unwrap())
            .reduce(|a, b| a / b)
            .ok_or(AstroPhiError::Internal)
    } else {
        shutterspeed.parse().map_err(AstroPhiError::ParseFloat)
    }
}

pub async fn run_command(Json(payload): Json<Command>) -> Result<Vec<u8>, AstroPhiError> {
    tracing::info!("POST /command: {:?}", payload);

    match payload {
        Command::Shoot { count } => {
            take_frames(count)?;
            Ok(vec![])
        }
        Command::Reset => {
            reset_total()?;
            Ok(vec![])
        }
        Command::Preview => take_preview(),
    }
}

fn take_frames(count: u32) -> Result<(), AstroPhiError> {
    if count == 0 {
        tracing::error!("Trying to shoot 0 frames");
        return Ok(());
    }
    let camera = Context::new()?.autodetect_camera().wait()?;
    for _ in 0..count {
        let file = camera.capture_image().wait()?;
        tracing::info!("Captured image {}", file.name());
        thread::sleep(time::Duration::from_millis(100));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(TEMP_FILE)?;
    let mut total_frames = TOTAL_FRAMES.lock().unwrap();
    tracing::info!(
        "Shot frames {} - {}",
        *total_frames,
        *total_frames + count - 1
    );
    *total_frames += count;
    file.write_all((*total_frames).to_string().as_bytes())?;

    Ok(())
}

fn reset_total() -> Result<(), AstroPhiError> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(TEMP_FILE)?;
    let mut total_frames = TOTAL_FRAMES.lock().unwrap();
    *total_frames = 0;
    file.write_all((*total_frames).to_string().as_bytes())?;
    tracing::info!("Reset temp file");

    Ok(())
}

fn take_preview() -> Result<Vec<u8>, AstroPhiError> {
    let context = Context::new()?;
    let camera = context.autodetect_camera().wait()?;
    let preview = camera.capture_preview().wait()?;
    let buf = preview.get_data(&context).wait()?;
    tracing::info!("Preview image taken");

    // let mut file = File::open("/tmp/preview_image.jpg")?;
    // let mut buf = Vec::new();
    // file.read_to_end(&mut buf)?;
    Ok(buf.to_vec())
}

pub async fn camera_config(Json(payload): Json<Config>) -> Result<String, AstroPhiError> {
    tracing::info!("POST /config: {:?}", payload);

    match payload {
        Config::Set { object, value } => set_config(object, value),
        Config::Get { object } => get_config(object),
    }
}

fn set_config(object: String, value: String) -> Result<String, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    match object.as_str() {
        "capturetarget" => {
            let capturetarget = camera.config_key::<RadioWidget>("capturetarget").wait()?;
            capturetarget.set_choice(&value)?;
            camera.set_config(&capturetarget).wait()?;
            tracing::info!("Config SET: {}:{}", object, value);

            Ok(format!("OK {}:{}", object, value))
        }
        _ => Err(AstroPhiError::Internal),
    }
}
fn get_config(object: String) -> Result<String, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    match object.as_str() {
        "capturetarget" => {
            let capturetarget = camera
                .config_key::<RadioWidget>("capturetarget")
                .wait()?
                .choice();
            tracing::info!("Config GET: {}:{}", object, capturetarget);

            Ok(capturetarget)
        }
        _ => Err(AstroPhiError::Internal),
    }
}

pub async fn get_logs() -> Result<String, StatusCode> {
    tracing::info!("GET /logs");

    let logs = fs::read_to_string(format!("{}/{}", LOG_DIR, LOG_FILE))
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(logs)
}
