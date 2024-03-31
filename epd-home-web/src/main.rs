use std::{env, io::Cursor};

use actix_web::{get, middleware, post, web, App, HttpResponse, HttpServer, Responder, ResponseError};
use epd_home::screen::{self, Screen};
use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Failed to create screen")]
    ScreenError(#[from] screen::Error),
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        use Error::*;
        
        match self {
            ScreenError(err) => {
                use screen::Error::*;
                match err {
                    InvalidTimezone => HttpResponse::BadRequest().into(),
                    _ => HttpResponse::BadGateway().into(),
                }
            }
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

#[get("/home.bmp")]
async fn get_home_screen(options: web::Query<HomeScreenOptions>) -> Result<impl Responder> {
    let mut out_buffer = Cursor::new(Vec::<u8>::new());

    Screen::new(options.lat, options.lon, &options.timezone, &options.stop_code)?
        .render(&mut out_buffer)
        .await?;

    let response = HttpResponse::Ok()
        .content_type("image/bmp")
        .body(out_buffer.into_inner());

    Ok(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or("127.0.0.1:8080".to_string());

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Compress::default())
            .service(ok)
            .service(get_home_screen)
    })
    .bind(listen_address)?
    .run()
    .await
}
