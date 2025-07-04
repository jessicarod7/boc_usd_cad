use clap::Parser;
use jiff::ToSpan;
use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::cmp::Ordering;

const BOC_BASE_URL: &str = "https://www.bankofcanada.ca/valet";

/// Get the USD to CAD exchange rate from the Bank of Canada for a single date, or a range.
///
/// Intended for adjusted cost basis calculations for tax purposes. Will return the preceding
/// business day if selected date is not available.
#[derive(Parser)]
pub struct Cli {
    /// A single date, or start date of the range (format: YYYY-MM-DD)
    #[arg(value_name = "DATE")]
    pub start_date: Date,
    /// End date of the range (format: YYYY-MM-DD)
    #[arg(value_name = "DATE")]
    pub end_date: Option<Date>,

    /// Provide the exchange rate from CAD to USD
    #[clap(short, long)]
    pub reverse: bool,
}

pub fn retrieve_rates(args: &Cli) -> Result<Vec<Observation>, String> {
    let request_url = match args.reverse {
        false => format!("{BOC_BASE_URL}/observations/FXUSDCAD/json"),
        true => format!("{BOC_BASE_URL}/observations/FXCADUSD/json"),
    };

    let request = ureq::get(request_url)
        // Retrieve previous 10 days to account for weekends and holidays
        .query("start_date", (args.start_date - 10.days()).to_string());

    let request = match args.end_date {
        None => request,
        Some(end_date) => {
            if end_date < args.start_date {
                panic!(
                    "end date {end_date} is before start date {}",
                    args.start_date
                );
            } else {
                request.query("end_date", end_date.to_string())
            }
        }
    };

    let mut resp = request.call().expect("failure while accessing BoC Valet");
    if resp.status().is_success() {
        Ok(filter_rates(
            args.start_date,
            args.end_date.is_some(),
            resp.body_mut()
                .read_json::<ObservationsResponse>()
                .expect("failed to parse exchange data")
                .observations,
        ))
    } else {
        Err(serde_json::to_string_pretty(
            &resp
                .body_mut()
                .read_json::<Value>()
                .unwrap_or_else(|e| panic!("non-JSON response ({}): {e}", resp.status())),
        )
        .unwrap()
        .to_string())
    }
}

/// Filter rates to the selected date range
fn filter_rates(
    start_date: Date,
    is_range: bool,
    mut observations: Vec<Observation>,
) -> Vec<Observation> {
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
        observations.into_iter().skip(range_start).collect()
    } else {
        vec![observations.into_iter().nth(range_start).unwrap()]
    }
}

#[derive(Deserialize)]
struct ObservationsResponse {
    observations: Vec<Observation>,
}

#[derive(Deserialize)]
pub struct Observation {
    pub d: Date,
    #[serde(rename = "FXUSDCAD", alias = "FXCADUSD")]
    pub fx: Fx,
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

#[derive(Deserialize)]
pub struct Fx {
    /// Value of 1 unit of left currency in right currency
    pub v: Decimal,
}

#[cfg(test)]
mod tests {
    use crate::{Cli, retrieve_rates};
    use jiff::civil::{Date, date};

    /// Test that the correct dates are returned for a series of inputs.
    #[test]
    fn test_date_ranges() {
        // Single business day
        assert_eq!(get_dates(date(2025, 1, 15), None), vec![date(2025, 1, 15)]);
        // Multiple business days
        assert_eq!(
            get_dates(date(2025, 1, 15), Some(date(2025, 1, 17))),
            vec![date(2025, 1, 15), date(2025, 1, 16), date(2025, 1, 17),]
        );

        // Single non-business day
        assert_eq!(get_dates(date(2025, 1, 18), None), vec![date(2025, 1, 17)]);
        // Multiple business days ending in a non-business day
        assert_eq!(
            get_dates(date(2025, 1, 15), Some(date(2025, 1, 18))),
            vec![date(2025, 1, 15), date(2025, 1, 16), date(2025, 1, 17),]
        );
        // Multiple non-business days, followed by multiple business days
        assert_eq!(
            get_dates(date(2025, 1, 18), Some(date(2025, 1, 21))),
            vec![date(2025, 1, 17), date(2025, 1, 20), date(2025, 1, 21),]
        );
        // Multiple business days, with multiple non-business days in between
        assert_eq!(
            get_dates(date(2025, 1, 15), Some(date(2025, 1, 21))),
            vec![
                date(2025, 1, 15),
                date(2025, 1, 16),
                date(2025, 1, 17),
                date(2025, 1, 20),
                date(2025, 1, 21),
            ]
        );
    }

    fn get_dates(start_date: Date, end_date: Option<Date>) -> Vec<Date> {
        let observations = retrieve_rates(&Cli {
            start_date,
            end_date,
            reverse: false,
        })
        .expect("failed to retrieve rates");
        observations.into_iter().map(|obs| obs.d).collect()
    }
}
