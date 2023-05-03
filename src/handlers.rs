use crate::{
    error::AstroPhiError, CameraInfo, Command, Config, LOG_DIR, LOG_FILE, TEMP_FILE, TOTAL_FRAMES,
};
use axum::{http::StatusCode, response::Html, Json};
use gphoto2::{widget::RadioWidget, Context};
use std::fs::File;
use std::io::Read;
use std::process;
use std::time::Duration;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use std::{thread, time};
use wait_timeout::ChildExt;

//const HTML_FILE: &str = "/var/www/index.html";
const HTML_FILE: &str = "static/index.html";

// Handler for the main page
pub async fn root() -> Result<Html<String>, StatusCode> {
    tracing::info!("GET root");

    let html = Html(fs::read_to_string(HTML_FILE).map_err(|_| StatusCode::NOT_FOUND)?);
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
        Command::Solve => {
            let capturetarget = get_config("capturetarget")?;
            let imageformat = get_config("imageformat")?;

            set_config("capturetarget", "Internal RAM")?;
            set_config("imageformat", "Smaller JPEG")?;

            let result = solve_plate();

            set_config("capturetarget", &capturetarget)?;
            set_config("imageformat", &imageformat)?;

            result
        }
        Command::Exposure => {
            let capturetarget = get_config("capturetarget")?;
            let imageformat = get_config("imageformat")?;

            set_config("capturetarget", "Internal RAM")?;
            set_config("imageformat", "Smaller JPEG")?;

            let result = take_exposure();

            set_config("capturetarget", &capturetarget)?;
            set_config("imageformat", &imageformat)?;

            result
        }
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

fn take_exposure() -> Result<Vec<u8>, AstroPhiError> {
    let context = Context::new()?;
    let camera = context.autodetect_camera().wait()?;

    let file = camera.capture_image().wait()?;
    let image = camera.fs().download(&file.folder(), &file.name()).wait()?;
    let buf = image.get_data(&context).wait()?;

    tracing::info!("Exposure taken");

    Ok(buf.to_vec())
}

fn solve_plate() -> Result<Vec<u8>, AstroPhiError> {
    let context = Context::new()?;
    let camera = context.autodetect_camera().wait()?;

    let file = camera.capture_image().wait()?;

    let image = camera.fs().download(&file.folder(), &file.name()).wait()?;
    let buf = image.get_data(&context).wait()?;

    let mut file = fs::File::create("solve.jpg")?;
    file.write_all(&buf)?;

    tracing::info!("Exposure taken");
    tracing::info!("Running astrometry");

    let mut child = process::Command::new("solve-field")
        .args(["--overwrite", "--downsample", "2", "solve.jpg"])
        .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin:/usr/local/astrometry/bin")
        .spawn()?;

    let status_code = match child.wait_timeout(Duration::from_secs(180))? {
        Some(status) => status.code(),
        None => {
            // child hasn't exited yet
            child.kill()?;
            child.wait()?;
            None
        }
    };

    if status_code != Some(0) {
        tracing::error!("Astrometry failed");
        return Err(AstroPhiError::Internal);
    }

    let mut file = File::open("solve-ngc.png")?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    tracing::info!("Solved plate");

    Ok(buf.to_vec())
}

pub async fn camera_config(Json(payload): Json<Config>) -> Result<String, AstroPhiError> {
    tracing::info!("POST /config: {:?}", payload);

    match payload {
        Config::Set { object, value } => set_config(&object, &value),
        Config::Get { object } => get_config(&object),
    }
}

fn set_config(object: &str, value: &str) -> Result<String, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    match object {
        "capturetarget" => {
            let capturetarget = camera.config_key::<RadioWidget>("capturetarget").wait()?;
            capturetarget.set_choice(value)?;
            camera.set_config(&capturetarget).wait()?;
        }
        "imageformat" => {
            let imageformat = camera.config_key::<RadioWidget>("imageformat").wait()?;
            imageformat.set_choice(value)?;
            camera.set_config(&imageformat).wait()?;
        }
        _ => return Err(AstroPhiError::Internal),
    }

    tracing::info!("Config SET: {}:{}", object, value);
    Ok(format!("OK {}:{}", object, value))
}

fn get_config(object: &str) -> Result<String, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    let config = match object {
        "capturetarget" => camera
            .config_key::<RadioWidget>("capturetarget")
            .wait()?
            .choice(),
        "imageformat" => camera
            .config_key::<RadioWidget>("imageformat")
            .wait()?
            .choice(),
        _ => return Err(AstroPhiError::Internal),
    };

    tracing::info!("Config GET: {}:{}", object, config);
    Ok(config)
}

pub async fn get_logs() -> Result<String, StatusCode> {
    tracing::info!("GET /logs");

    let logs = fs::read_to_string(format!("{}/{}", LOG_DIR, LOG_FILE))
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(logs)
}
