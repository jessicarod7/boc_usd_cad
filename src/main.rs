use std::cmp::Ordering;

use clap::Parser;
use jiff::ToSpan;
use jiff::civil::Date;
use num_traits::Inv;
use reqwest::blocking::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const BOC_BASE_URL: &str = "https://www.bankofcanada.ca/valet";

/// Get the USD to CAD exchange rate from the Bank of Canada for a single date, or a range.
///
/// Intended for adjusted cost basis calculations for tax purposes. Will return the preceding
/// business day if selected date is not available.
#[derive(Parser)]
struct Cli {
    /// A single date, or start date of the range (format: YYYY-MM-DD)
    #[arg(value_name = "DATE")]
    start_date: Date,
    /// End date of the range (format: YYYY-MM-DD)
    #[arg(value_name = "DATE")]
    end_date: Option<Date>,

    /// Provide the exchange rate from CAD to USD
    #[clap(short, long)]
    reverse: bool,
}

fn main() {
    let args = Cli::parse();

    let request = Client::new()
        .get(format!("{BOC_BASE_URL}/observations/FXUSDCAD/json"))
        // Retrieve previous 10 days to account for weekends and holidays
        .query(&[("start_date", args.start_date - 10.days())]);

    let request = match args.end_date {
        None => request,
        Some(end_date) => {
            if end_date < args.start_date {
                panic!(
                    "end date {end_date} is before start date {}",
                    args.start_date
                );
            } else {
                request.query(&[("end_date", end_date)])
            }
        }
    };

    let resp = request.send().expect("failure while accessing BoC Valet");
    if resp.status().is_success() {
        let mut observations = resp
            .json::<ObservationsResponse>()
            .expect("failed to parse exchange data")
            .observations;

        if args.reverse {
            for obs in observations.iter_mut() {
                obs.fxusdcad.v = obs.fxusdcad.v.inv().round_dp(4);
            }
        }

        print_results(args.start_date, args.end_date.is_some(), observations);
    } else {
        let status = resp.status();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &resp
                    .json::<Value>()
                    .unwrap_or_else(|e| { panic!("non-JSON response ({}): {e}", status) })
            )
            .unwrap()
        );
    }
}

fn print_results(start_date: Date, is_range: bool, mut observations: Vec<Observation>) {
    // Index of the start date, defined as the specified start date, or the last day before it with data
    observations.sort_unstable();
    let range_start = observations
        .iter()
        .enumerate()
        .filter(|(_, obs)| obs.d <= start_date)
        .max_by_key(|(_, obs)| obs.d)
        .unwrap()
        .0;

    if is_range {
        for obs in observations.into_iter().skip(range_start) {
            println!("{}: {}", obs.d, obs.fxusdcad.v)
        }
    } else {
        let obs = observations.into_iter().nth(range_start).unwrap();
        println!("{}: {}", obs.d, obs.fxusdcad.v)
    }
}

#[derive(Deserialize)]
struct ObservationsResponse {
    observations: Vec<Observation>,
}

#[derive(Deserialize)]
struct Observation {
    d: Date,
    #[serde(rename = "FXUSDCAD")]
    fxusdcad: FxUsdCad,
}

#[derive(Deserialize)]
struct FxUsdCad {
    /// Value of USD 1 in CAD
    v: Decimal,
}

impl PartialEq<Self> for Observation {
    fn eq(&self, other: &Self) -> bool {
        self.d.eq(&other.d)
    }
}

impl Eq for Observation {}

impl PartialOrd for Observation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Observation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.d.cmp(&other.d)
    }
}
