use anyhow::Result;
use std::fs::File;

use epd_home::screen::Screen;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let mut out_file = File::create("home.bmp")?;

    Screen::new(-36.75, 174.625, "Pacific/Auckland", "3889")?
        .render(&mut out_file)
        .await?;

    Ok(())
}
