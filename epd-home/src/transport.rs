use askama::Template;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Stop {
    pub id: String,
    pub code: String,
    pub name: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopArrival {
    pub trip_id: String,
    pub stop_sequence: u32,
    pub start_timestamp: i64,
    pub arrival_timestamp: i64,
    pub updated_arrival_timestamp: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct RouteTrip {
    pub route_id: String,
    pub route_short_name: String,
    pub route_long_name: String,
    pub route_type: i32,
    pub route_color: String,
    pub route_text_color: String,
    pub stop_headsign: String,
}

#[derive(Deserialize, Debug)]
pub struct StopRouteTripArrival {
    pub route_trip: RouteTrip,
    pub arrivals: Vec<StopArrival>,
}

#[derive(Deserialize)]
pub struct StopsResponse {
    pub stops: Vec<Stop>,
}

#[derive(Deserialize)]
pub struct StopArrivalsResponse {
    pub stop_arrivals: Vec<StopRouteTripArrival>,
}

#[derive(Template)]
#[template(
    source = "https://next-at-api.heaps.dev/stops?code={{ stop_code|urlencode_strict }}",
    ext = "txt"
)]
struct StopsUrl {
    stop_code: String,
}

#[derive(Template)]
#[template(
    source = "https://next-at-api.heaps.dev/stops/{{ stop_id|urlencode_strict }}/arrivals",
    ext = "txt"
)]
struct ArrivalsUrl {
    stop_id: String,
}

pub async fn get_stop_arrivals(
    stop_code: &str,
) -> Result<Option<Vec<StopRouteTripArrival>>, reqwest::Error> {
    let stops_url = StopsUrl {
        stop_code: stop_code.to_string(),
    }
    .render()
    .unwrap();
    let stops = reqwest::get(&stops_url)
        .await?
        .json::<StopsResponse>()
        .await?
        .stops;

    if let Some(stop) = stops.first() {
        let arrivals_url = ArrivalsUrl {
            stop_id: stop.id.clone(),
        }
        .render()
        .unwrap();
        let arrivals = reqwest::get(&arrivals_url)
            .await?
            .json::<StopArrivalsResponse>()
            .await?
            .stop_arrivals;

        Ok(Some(arrivals))
    } else {
        Ok(None)
    }
}
