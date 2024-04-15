use std::{env, io::Cursor};

use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder, ResponseError};
use epd_home::screen::{self, Screen};
use image::{codecs::qoi::QoiEncoder, ImageEncoder};
use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Failed to create screen")]
    Screen(#[from] screen::Error),

    #[error("Failed to decode image")]
    Image(#[from] image::error::ImageError),

    #[error("Failed to encode image")]
    Bitmap(#[from] bmp_monochrome::BmpError),
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        use Error::*;

        log::error!("Error: {:?}", self);
        
        match self {
            Screen(err) => {
                use screen::Error::*;
                match err {
                    InvalidTimezone => HttpResponse::BadRequest().into(),
                    _ => HttpResponse::BadGateway().into(),
                }
            },
            _ => HttpResponse::InternalServerError().into(),
        }
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[get("/ok")]
async fn ok() -> impl Responder {
    HttpResponse::Ok()
}

#[derive(Deserialize)]
struct HomeScreenOptions {
    lat: f64,
    lon: f64,
    timezone: String,
    stop_code: String,
}

async fn render(options: web::Query<HomeScreenOptions>) -> Result<Vec<Vec<bool>>> {
    let stop_codes: Vec<String> = options.stop_code.split(',').map(|s| s.to_string()).collect();
    let stop_codes_ref = stop_codes.iter().map(|s| s.as_str()).collect::<Vec<&str>>();

    let img = Screen::new(options.lat, options.lon, &options.timezone, &stop_codes_ref)?
        .render()
        .await?;

    Ok(img)
}

#[get("/home.bmp")]
async fn get_home_screen_bmp(options: web::Query<HomeScreenOptions>) -> Result<impl Responder> {
    
    let data = render(options).await?;

    let mut buff = Cursor::new(Vec::new());
    bmp_monochrome::Bmp::new(data)?.write(&mut buff)?;

    let response = HttpResponse::Ok()
        .content_type("image/bmp")
        .body(buff.into_inner());

    Ok(response)
}

#[get("/home.qoi")]
async fn get_home_screen_qoi(options: web::Query<HomeScreenOptions>) -> Result<impl Responder> {
    
    let img = render(options).await?;

    let w = img[0].len() as u32;
    let h = img.len() as u32;

    let data: Vec<u8> = img.into_iter().flatten().flat_map(|px| {
        if px {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        }
    })
    .collect();

    let mut buff = Cursor::new(Vec::new());
    QoiEncoder::new(&mut buff)
        .write_image(&data, w, h, image::ExtendedColorType::Rgb8)?;

    let response = HttpResponse::Ok()
        .content_type("image/qoi")
        .body(buff.into_inner());

    Ok(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or("127.0.0.1:8080".to_string());

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Compress::default())
            .service(ok)
            .service(get_home_screen_bmp)
            .service(get_home_screen_qoi)
    })
    .bind(listen_address)?
    .run()
    .await
}
