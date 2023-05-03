use axum::{
    routing::{get, post},
    Router,
};
use handlers::*;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::File,
    io::{Read, Write},
    net::SocketAddr,
    sync::Mutex,
};

mod error;
mod handlers;

pub const LOG_DIR: &str = "/var/log";
pub const LOG_FILE: &str = "astrophi.log";
pub const TEMP_FILE: &str = "./astrophi_temp";

pub static WORK_DIR: Mutex<&str> = Mutex::new("/tmp");
pub static TOTAL_FRAMES: Mutex<u32> = Mutex::new(0);

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: ./astrophi [temp_directory] [port]");
        std::process::exit(1);
    }

    let port: u16 = args[2].parse().expect("Invalid port");
    {
        if !std::path::Path::new(&args[1]).is_dir() {
            println!("Invalid work directory");
            std::process::exit(1);
        }
        let mut work_dir = WORK_DIR.lock().unwrap();
        *work_dir = Box::leak(args[1].clone().into_boxed_str());
    }

    let file_appender = tracing_appender::rolling::never(LOG_DIR, LOG_FILE);

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_writer(file_appender)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let app = Router::new()
        .route("/", get(root))
        .route("/info", get(camera_info))
        .route("/command", post(run_command))
        .route("/config", post(camera_config))
        .route("/logs", get(get_logs));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {}", addr);

    {
        let work_dir = WORK_DIR.lock().unwrap();
        env::set_current_dir(*work_dir).unwrap();
        tracing::info!("Work dir: {}", *work_dir);
    }

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

#[derive(Serialize, Deserialize, Debug)]
pub struct CameraInfo {
    pub iso: u32,
    pub aperture: f32,
    pub exposure: f32,
    pub capturetarget: String,
    pub total_frames: u32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Command {
    Shoot { count: u32 },
    Reset,
    Preview,
    Solve,
    Exposure,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Config {
    Set { object: String, value: String },
    Get { object: String },
}
