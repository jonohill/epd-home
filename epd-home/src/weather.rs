use reqwest::Url;
use serde::Deserialize;

// https://open-meteo.com/en/docs
const WEATHER_URL: &str = "https://api.open-meteo.com/v1/forecast?current=temperature_2m,is_day,weather_code,wind_gusts_10m&hourly=temperature_2m,weather_code,is_day,wind_gusts_10m&daily=sunrise,sunset&forecast_days=2&timezone=UTC";

#[derive(Deserialize, Debug)]
pub struct MeteoWeather {
    pub hourly: MeteoHourlyWeather,
    pub daily: MeteoDailyWeather,
    pub current: MeteoCurrentWeather,
}

#[derive(Deserialize, Debug)]
pub struct MeteoCurrentWeather {
    pub time: String,
    pub is_day: u8,
    pub temperature_2m: f64,
    pub weather_code: u32,
    pub wind_gusts_10m: f64,
}

#[derive(Deserialize, Debug)]
pub struct MeteoHourlyWeather {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub is_day: Vec<u8>,
    pub wind_gusts_10m: Vec<f64>,
}

#[derive(Deserialize, Debug)]
pub struct MeteoDailyWeather {
    pub sunrise: Vec<String>,
    pub sunset: Vec<String>,
}

#[derive(Debug)]
pub struct Weather {
    pub current: WeatherForecast,
    pub forecast: Vec<WeatherForecast>,
    pub sunrises: Vec<String>,
    pub sunsets: Vec<String>,
}

#[derive(Debug)]
pub struct WeatherForecast {
    pub time: String,
    pub weather_code: u32,
    pub temperature: f64,
    pub is_day: bool,
    pub wind_gusts: f64,
}

pub async fn fetch_weather(latitude: f64, longitude: f64) -> Result<Weather, reqwest::Error> {
    let mut url = Url::parse(WEATHER_URL).unwrap();
    url.query_pairs_mut()
        .append_pair("latitude", &latitude.to_string())
        .append_pair("longitude", &longitude.to_string());

    let response: MeteoWeather = reqwest::get(url).await?.error_for_status()?.json().await?;

    let current = WeatherForecast {
        time: response.current.time,
        weather_code: response.current.weather_code,
        temperature: response.current.temperature_2m as f64,
        is_day: response.current.is_day == 1,
        wind_gusts: response.current.wind_gusts_10m,
    };

    let mut forecast = vec![];

    for i in 0..response.hourly.time.len() {
        forecast.push(WeatherForecast {
            time: response.hourly.time[i].clone(),
            weather_code: response.hourly.weather_code[i],
            temperature: response.hourly.temperature_2m[i],
            is_day: response.hourly.is_day[i] == 1,
            wind_gusts: response.hourly.wind_gusts_10m[i],
        });
    }

    log::debug!("{:?}", forecast);

    let weather = Weather {
        current,
        forecast,
        sunrises: response.daily.sunrise,
        sunsets: response.daily.sunset,
    };
    Ok(weather)
}
