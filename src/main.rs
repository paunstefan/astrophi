use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use error::AstroPhiError;
use gphoto2::{widget::RadioWidget, Context};
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::SocketAddr,
    sync::Mutex,
};
use std::{thread, time};

mod error;

const HOME: &'static str = "/home/paunstefan";
const TEMP_FILE: &'static str = "./astrophi_temp";

static TOTAL_FRAMES: Mutex<u32> = Mutex::new(0);

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let app = Router::new()
        .route("/", get(root))
        .route("/info", get(camera_info))
        .route("/command", post(run_command))
        .route("/config", post(camera_config));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);

    // TODO: uncomment for release
    //env::set_current_dir(HOME).unwrap();

    if !std::path::Path::new(TEMP_FILE).exists() {
        tracing::info!("Creating temporary file: {}", TEMP_FILE);
        let mut file = File::create(TEMP_FILE).expect("Error creating temporary file");
        file.write_all(b"0")
            .expect("Error writing to temporary file");
    } else {
        tracing::info!("Reading temporary file: {}", TEMP_FILE);
        let mut file = File::open(TEMP_FILE).expect("Error opening temporary file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("Error reading temporary file");
        let count: u32 = contents.parse().expect("Error parsing temporary file");
        tracing::info!("Total frames read: {}", count);

        let mut total = TOTAL_FRAMES.lock().unwrap();
        *total = count;
    }

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
    pub total_frames: u32,
}

async fn camera_info() -> Result<Json<CameraInfo>, AstroPhiError> {
    let camera = Context::new()?.autodetect_camera().wait()?;

    let iso: u32 = camera
        .config_key::<RadioWidget>("iso")
        .wait()?
        .choice()
        .parse()?;
    tracing::debug!("Parsed ISO: {:?}", iso);

    let exposure: f32 = parse_shutterspeed(
        &camera
            .config_key::<RadioWidget>("shutterspeed")
            .wait()?
            .choice(),
    )?;
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
        shutterspeed
            .parse()
            .map_err(|e| AstroPhiError::ParseFloat(e))
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Command {
    Shoot { count: u32 },
    Reset,
    Preview,
}

async fn run_command(Json(payload): Json<Command>) -> Result<Vec<u8>, AstroPhiError> {
    println!("{:?}", payload);
    match payload {
        Command::Shoot { count } => {
            take_frames(count)?;
            Ok(vec![])
        }
        Command::Reset => {
            reset_total()?;
            Ok(vec![])
        }
        Command::Preview => {
            tracing::error!("Preview not yet available");

            Ok(vec![])
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
    tracing::info!("Frames {} - {}", *total_frames, *total_frames + count - 1);
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

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Config {
    Set { object: String, value: String },
    Get { object: String },
}

async fn camera_config(Json(payload): Json<Config>) -> Result<String, AstroPhiError> {
    println!("{:?}", payload);

    match payload {
        Config::Set { object, value } => todo!(),
        Config::Get { object } => todo!(),
    }
}

fn set_config(object: String, value: String) -> Result<String, AstroPhiError> {
    match object.as_str() {
        "capturetarget" => todo!(),
        _ => Err(AstroPhiError::Internal),
    }
}
fn get_config(object: String) -> Result<String, AstroPhiError> {
    match object.as_str() {
        "capturetarget" => todo!(),
        _ => Err(AstroPhiError::Internal),
    }
}
