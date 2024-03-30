use std::io::Cursor;

use epd_home::screen::Screen;
use worker::*;

#[event(start)]
fn start() {
    console_log::init_with_level(log::Level::Debug).unwrap();
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {

    let mut buffer = Cursor::new(Vec::<u8>::new());
    
    Screen::new(-36.75, 174.625, "Pacific/Auckland", "3889").unwrap()
        .render(&mut buffer)
        .await
        .unwrap();

    let mut headers = Headers::default();
    headers.append("Content-Type", "image/bmp").unwrap();

    let resp = Response::from_bytes(buffer.into_inner()).unwrap()
        .with_headers(headers);

    Ok(resp)
}
