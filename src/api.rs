use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref MAIN_PATTERN: Regex = Regex::new("Parametersatz = \"([^\"]*)\"").unwrap();
    static ref UCELL_STATS_PATTERN: Regex = Regex::new("PSet0 = \"([^\"]*)\"").unwrap();
    static ref UCELL_CELLS_PATTERN: Regex = Regex::new("PSet = \"([^\"]*)\"").unwrap();
}

pub enum Error {
    Unexpected,
    Fetch(anyhow::Error),
}

#[derive(Default)]
pub struct Data {
    pub main: Main,
    pub ucell: Ucell,
}

#[derive(Default)]
pub struct Main {
    // in mV
    pub voltage: u32,
    // in mA
    pub current: f32,
    // in %
    pub state_of_charge: f32,
    // in °C
    pub temp_avg: f32,
    pub temp_min: f32,
    pub temp_max: f32,
    pub temp_master: f32,
}

#[derive(Default)]
pub struct Ucell {
    pub num_cells: usize,
    pub num_slaves: usize,
    pub num_cells_per_slave: usize,
    pub num_temp_sensors: usize,
    pub num_safe_resistors: usize,

    // in mV
    pub avg_voltage: u16,
    pub min_voltage: u16,
    pub max_voltage: u16,
    pub cell_voltage: Vec<u16>,
}

pub fn fetch(ip: &str) -> anyhow::Result<Data> {
    Ok(Data {
        main: Main::default(),
        ucell: ucell(ip)?,
    })
}

async fn main_data(ip: &str) -> anyhow::Result<Main> {
    let url = format!("http://{ip}/main_data.shtml");
    let resp = ureq::get(&url).call()?;
    let text = resp.into_string()?;

    let stats_captures = MAIN_PATTERN.captures(&text).unwrap();
    let mut stats_iter = stats_captures.get(1).unwrap().as_str().split(',');

    skip(&mut stats_iter, 1);
    let voltage = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let current = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let state_of_charge = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let temp_avg = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let temp_min = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let temp_max = parse_next(&mut stats_iter)?;
    skip(&mut stats_iter, 2);
    let temp_master = parse_next(&mut stats_iter)?;

    Ok(Main {
        voltage,
        current,
        state_of_charge,
        temp_avg,
        temp_min,
        temp_max,
        temp_master,
    })
}

fn ucell(ip: &str) -> anyhow::Result<Ucell> {
    let url = format!("http://{ip}/ucell.shtml");
    let resp = ureq::get(&url).call()?;
    let text = resp.into_string()?;

    let voltage_captures = UCELL_CELLS_PATTERN.captures(&text).unwrap();
    let voltage = voltage_captures
        .get(1)
        .unwrap()
        .as_str()
        .split(',')
        .skip(2)
        .map(|s| s.parse::<u16>().unwrap_or(0))
        .collect();

    let stats_captures = UCELL_STATS_PATTERN.captures(&text).unwrap();
    let mut stats_iter = stats_captures.get(1).unwrap().as_str().split(',');

    Ok(Ucell {
        num_cells: parse_next(&mut stats_iter)?,
        num_slaves: parse_next(&mut stats_iter)?,
        num_cells_per_slave: parse_next(&mut stats_iter)?,
        num_temp_sensors: parse_next(&mut stats_iter)?,
        num_safe_resistors: parse_next(&mut stats_iter)?,

        avg_voltage: parse_next(&mut stats_iter)?,
        min_voltage: parse_next(&mut stats_iter)?,
        max_voltage: parse_next(&mut stats_iter)?,
        cell_voltage: voltage,
    })
}

fn parse_next<'a, T: FromStr>(iter: &mut impl Iterator<Item = &'a str>) -> anyhow::Result<T> {
    match iter.next() {
        Some(s) => s
            .parse()
            .map_err(|_| anyhow::anyhow!("Error parsing value")),
        None => anyhow::bail!("Value not found"),
    }
}

fn skip(iter: &mut impl Iterator, skip: usize) {
    for _ in 0..skip {
        iter.next();
    }
}
