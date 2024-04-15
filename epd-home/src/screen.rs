use core::fmt;
use std::{fmt::Display, io::Write, path::Path, sync::Arc};

use askama::Template;
use chrono::{DateTime, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use fontdb::Source;
use itertools::Itertools;
use serde::Serialize;
use tiny_skia::{Color, Pixmap};
use tokio::join;
use usvg::{ImageHrefResolver, ImageKind};
use crate::dither::{ditherer::STUCKI, prelude::*};

use crate::{transport::get_stop_arrivals, weather::fetch_weather};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid timezone")]
    InvalidTimezone,

    #[error("Failed to fetch: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Missing data: {0}")]
    MissingData(String),

    #[error(transparent)]
    InvalidDateFormat(#[from] chrono::ParseError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Icon {
    Sun,
    Cloud,
    CloudDrizzle,
    CloudRain,
    CloudSnow,
    CloudLightning,
    Moon,
    Sunrise,
    Sunset,
    Wind,
}

impl Display for Icon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        serde_json::to_value(self).unwrap().as_str().unwrap().fmt(f)
    }
}

fn icon_for_weather(code: u32, is_night: bool, gust_speed: f64) -> Icon {
    use Icon::*;

    let icon = match code {
        0..=1 => {
            if is_night {
                Moon
            } else {
                Sun
            }
        }
        2..=3 | 45..=48 => Cloud,
        51..=57 => CloudDrizzle,
        61..=67 | 80..=86 => CloudRain,
        71..=77 => CloudSnow,
        95..=99 => CloudLightning,
        _ => Cloud,
    };

    // if wind >= "strong" on the Beaufort scale
    if gust_speed >= 39.0 && [Moon, Sun, Cloud].contains(&icon) {
        Wind
    } else {
        icon
    }
}

#[derive(Clone)]
struct WeatherData {
    time: DateTime<Tz>,
    weather: Icon,
    temp: Option<String>,
}

#[derive(Debug)]
enum ArrivalTime {
    Now,
    Minutes(u32),
    Time(DateTime<Tz>),
}

impl ArrivalTime {
    fn minutes(&self, relative_to: &DateTime<Tz>) -> u32 {
        match self {
            ArrivalTime::Now => 0,
            ArrivalTime::Minutes(min) => *min,
            ArrivalTime::Time(dt) => {
                let delta = *dt - relative_to;
                *[delta.num_minutes() as u32, 0].iter().max().unwrap()
            }
        }
    }
}

#[derive(Debug)]
struct ArrivalData {
    route: String,
    headsign: String,
    arrival_times: Vec<ArrivalTime>,
}

mod filters {
    use chrono::DateTime;
    use chrono_tz::Tz;
    use titlecase::titlecase as title_case;

    pub fn formatdate(date: &DateTime<Tz>, format: &str) -> ::askama::Result<String> {
        Ok(date.format(format).to_string())
    }

    pub fn titlecase(input: &str) -> ::askama::Result<String> {
        Ok(title_case(input))
    }
}

#[derive(Template)]
#[template(path = "home.svg")]
struct HomeSvgTemplate {
    weather_now: Icon,
    temp_now: String,
    time: DateTime<Tz>,
    forecast: Vec<WeatherData>,
    arrivals: Vec<ArrivalData>,
}


fn save_to_image_bytes(pixmap: Pixmap) -> Vec<Vec<bool>> {
    // https://gitlab.com/efronlicht/dither/-/blob/master/src/bin/dither.rs?ref_type=heads

    let width = pixmap.width();
    let pixels = pixmap.pixels().iter().map(|p| {
        RGB(p.red(), p.green(), p.blue())
    });

    let img: Img<f64> = Img::<RGB<u8>>::new(pixels, width).unwrap()
        .convert_with(|rgb| rgb.convert_with(f64::from))
        .convert_with(|rgb| rgb.to_chroma_corrected_black_and_white());

    let quantize = create_quantize_n_bits_func(1).unwrap();

    let output = STUCKI.dither(img, quantize);

    let data = (0..pixmap.height())
        .map(|y| {
            (0..pixmap.width())
                .map(|x| {
                    let p = output.get((x, y)).unwrap();
                    *p == 0.0
                })
                .collect_vec()
        })
        .collect_vec();
    
    data
}

fn load_icon(icon_name: &str) -> Option<Vec<u8>> {
    match icon_name {
        "cloud-drizzle.svg" => Some(include_bytes!("../assets/icons/cloud-drizzle.svg").to_vec()),
        "cloud-lightning.svg" => Some(include_bytes!("../assets/icons/cloud-lightning.svg").to_vec()),
        "cloud-rain.svg" => Some(include_bytes!("../assets/icons/cloud-rain.svg").to_vec()),
        "cloud-snow.svg" => Some(include_bytes!("../assets/icons/cloud-snow.svg").to_vec()),
        "cloud.svg" => Some(include_bytes!("../assets/icons/cloud.svg").to_vec()),
        "moon.svg" => Some(include_bytes!("../assets/icons/moon.svg").to_vec()),
        "sun.svg" => Some(include_bytes!("../assets/icons/sun.svg").to_vec()),
        "sunrise.svg" => Some(include_bytes!("../assets/icons/sunrise.svg").to_vec()),
        "sunset.svg" => Some(include_bytes!("../assets/icons/sunset.svg").to_vec()),
        "wifi-off.svg" => Some(include_bytes!("../assets/icons/wifi-off.svg").to_vec()),
        "wind.svg" => Some(include_bytes!("../assets/icons/wind.svg").to_vec()),
        _ => None,
    }
}

async fn render_svg(svg_data: Vec<u8>) -> Vec<Vec<bool>> {
    // Based on https://github.com/RazrFalcon/resvg/blob/master/crates/resvg/examples/minimal.rs

    log::debug!("Make SVG tree");

    let tree = {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_font_source(Source::Binary(Arc::new(include_bytes!("../assets/Chivo-VariableFont_wght.ttf"))));
        fontdb.load_font_source(Source::Binary(Arc::new(include_bytes!("../assets/ChivoMono-VariableFont_wght.ttf"))));

        let dir = Path::new("assets").to_path_buf();

        let opt = usvg::Options {
            resources_dir: Some(dir),
            image_href_resolver: ImageHrefResolver {
                resolve_string: Box::new(move |href, opts, fontdb| {
                    if let Some(name) = href.strip_prefix("icons/") {
                        if let Some(icon) = load_icon(name) {
                            return Some(ImageKind::SVG(usvg::Tree::from_data(&icon, opts, fontdb).unwrap()));
                        }
                    }
                    None
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        usvg::Tree::from_data(&svg_data, &opt, &fontdb).unwrap()
    };

    log::debug!("Render SVG");

    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    pixmap.fill(Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    save_to_image_bytes(pixmap)
}

fn parse_weather_time(time: &str, tz: &Tz) -> Result<DateTime<Tz>, chrono::ParseError> {
    let parsed_dt = NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M")?;
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(parsed_dt, Utc).with_timezone(tz);
    Ok(dt)
}

pub struct Screen {
    latitude: f64,
    longitude: f64,
    timezone: Tz,
    stop_codes: Vec<String>,
}

impl Screen {
    pub fn new(latitude: f64, longitude: f64, timezone: &str, stop_codes: &[&str]) -> Result<Self> {
        let tz = timezone.parse().map_err(|_| Error::InvalidTimezone)?;

        let screen = Self {
            latitude,
            longitude,
            timezone: tz,
            stop_codes: stop_codes.iter().map(|s| s.to_string()).collect(),
        };
        Ok(screen)
    }

    fn parse_weather_time(&self, time: &str) -> Result<DateTime<Tz>, chrono::ParseError> {
        parse_weather_time(time, &self.timezone)
    }

    async fn gather_weather(&self) -> Result<(WeatherData, Vec<WeatherData>)> {
        let weather = fetch_weather(self.latitude, self.longitude).await?;

        let now = Utc::now().with_timezone(&self.timezone);

        let current = WeatherData {
            time: now,
            weather: icon_for_weather(
                weather.current.weather_code,
                !weather.current.is_day,
                weather.current.wind_gusts,
            ),
            temp: Some(weather.current.temperature.round().to_string()),
        };

        let hourly_data = weather
            .forecast
            .iter()
            .map(|data| {
                let d = WeatherData {
                    time: parse_weather_time(&data.time, &self.timezone)?,
                    weather: icon_for_weather(data.weather_code, !data.is_day, data.wind_gusts),
                    temp: Some(data.temperature.round().to_string()),
                };
                Ok::<_, Error>(d)
            })
            .collect::<Result<Vec<_>>>()?;

        let sunrise_sunsets = weather
            .sunrises
            .iter()
            .map(|time| (Icon::Sunrise, self.parse_weather_time(time)))
            .chain(
                weather
                    .sunsets
                    .iter()
                    .map(|time| (Icon::Sunset, self.parse_weather_time(time))),
            )
            .map(|(icon, time)| {
                time.map(|time| WeatherData {
                    time,
                    weather: icon,
                    temp: None,
                })
            })
            .sorted_by_key(|data| data.clone().map_or(now, |data| data.time))
            .collect::<Result<Vec<_>, chrono::ParseError>>()?;

        let mut forecast = vec![];

        const FORECAST_HOURS: usize = 2;
        const FORECAST_ROWS: usize = 4;

        let mut forecast_start = now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        let mut sun_changed = false;

        for _ in 0..FORECAST_ROWS {
            let forecast_end =
                forecast_start + chrono::Duration::try_hours(FORECAST_HOURS as i64).unwrap();

            if !sun_changed {
                let sun_change = sunrise_sunsets.iter().find(|data| {
                    data.time >= now && data.time >= forecast_start && data.time <= forecast_end
                });
                if let Some(sun_change) = sun_change {
                    forecast.push(sun_change.clone());
                    sun_changed = true;
                    continue;
                }
            }

            let hour_data = hourly_data
                .iter()
                .rev()
                .find(|data| data.time <= forecast_end);
            if let Some(hour_data) = hour_data {
                forecast.push(hour_data.clone());
            }

            sun_changed = false;
            forecast_start += chrono::Duration::try_hours(FORECAST_HOURS as i64).unwrap();
        }

        Ok((current, forecast))
    }

    async fn gather_arrivals(&self) -> Result<Vec<ArrivalData>> {
        let pending_arrivals = self.stop_codes.iter().map(|code| get_stop_arrivals(code)).collect_vec();
        let arrivals: Vec<_> = futures::future::join_all(pending_arrivals).await.into_iter()
            .filter_map(|item| item.transpose())
            .try_collect()?;
        let arrivals = arrivals.into_iter().flatten().collect_vec();

        let now = Utc::now().with_timezone(&self.timezone);

        let data: Vec<_> = arrivals
            .into_iter()
            .filter(|arr| !arr.arrivals.is_empty())
            .map(|arr| {
                let data = ArrivalData {
                    route: arr.route_trip.route_short_name,
                    headsign: arr.route_trip.stop_headsign,
                    arrival_times: arr
                        .arrivals
                        .into_iter()
                        .take(2)
                        .map(|arr_time| {
                            let time = arr_time
                                .updated_arrival_timestamp
                                .unwrap_or(arr_time.arrival_timestamp);
                            let dt = Utc
                                .timestamp_millis_opt(time)
                                .single()
                                .unwrap()
                                .with_timezone(&self.timezone);
                            let delta = dt - now;

                            let arrival_time = match delta {
                                d if d.num_minutes() < 1 => ArrivalTime::Now,
                                d if d.num_minutes() < 100 => {
                                    ArrivalTime::Minutes(d.num_minutes() as u32)
                                }
                                _ => ArrivalTime::Time(dt),
                            };
                            Ok(arrival_time)
                        }) // only take a single time arrival
                        .take_while_inclusive(|time| {
                            matches!(time, Ok(ArrivalTime::Now) | Ok(ArrivalTime::Minutes(_)))
                        })
                        .collect::<Result<Vec<_>>>()?,
                };
                Ok::<_, Error>(data)
            })
            .try_collect()?;

        let data = data
            .into_iter()
            // initial sort by arrival because we can only take 4
            .map(|data| {
                (
                    data.arrival_times
                        .iter()
                        .map(|time| time.minutes(&now))
                        .min()
                        .unwrap(),
                    data,
                )
            })
            .sorted_by_key(|(time, _)| *time)
            .take(4)
            // Then attempt to order in a way to minimise redraws
            // Everything an hour away or less first, then route, then headsign
            .sorted_by_key(|(min_time, data)| {
                let time = *min_time > 60;
                let route = data.route.clone();
                let headsign = data.headsign.clone();
                (time, route, headsign)
            })
            .map(|(_, data)| data)
            .collect::<Vec<_>>();

        Ok(data)
    }

    pub async fn render(&self) -> Result<Vec<Vec<bool>>> {
        let (weather, transport) = join!(self.gather_weather(), self.gather_arrivals());

        let (current_weather, forecast) = weather?;
        let arrivals = transport?;
        log::debug!("{:?}", arrivals);

        let svg_data: Vec<u8> = HomeSvgTemplate {
            weather_now: current_weather.weather,
            temp_now: current_weather
                .temp
                .map(|temp| temp.to_string())
                .ok_or_else(|| Error::MissingData("current temperature".into()))?,
            time: current_weather.time,
            forecast,
            arrivals,
        }
        .render()
        .unwrap()
        .into();

        log::debug!("SVG data: {}", String::from_utf8_lossy(&svg_data));

        let img_data = render_svg(svg_data).await;

        Ok(img_data)
    }

    pub async fn render_placeholder<T: Write>(&self) -> Result<Vec<Vec<bool>>> {
        let fake_now = Utc::now().with_timezone(&self.timezone).with_hour(12).unwrap().with_minute(0).unwrap();

        let svg_data: Vec<u8> = HomeSvgTemplate {
            weather_now: Icon::Cloud,
            temp_now: "-".into(),
            time: fake_now,
            forecast: (1..=4).map(|n| {
                WeatherData {
                    time: fake_now.with_hour(n * 2 + 12).unwrap(),
                    weather: Icon::Cloud,
                    temp: Some("-".into())
                }
            }).collect_vec(),
            arrivals: (1..=4).map(|n| {
                ArrivalData {
                    route: "---".into(),
                    headsign: "----------".into(),
                    arrival_times: vec![ArrivalTime::Minutes(n * 10)]
                }
            }).collect_vec()
        }
        .render()
        .unwrap()
        .into();

        let data = render_svg(svg_data).await;

        Ok(data)
    }

    pub async fn render_error<T: Write>(&self) -> Result<Vec<Vec<bool>>> {
        let svg_data: Vec<u8> = include_bytes!("../assets/error.svg").into();

        let data = render_svg(svg_data).await;

        Ok(data)
    }

}
