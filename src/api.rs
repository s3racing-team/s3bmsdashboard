use std::cmp;
use std::str::{FromStr, Split};
use std::thread::{self, JoinHandle};

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref MAIN_PATTERN: Regex = Regex::new("Parametersatz = \"([^\"]*)\"").unwrap();
    static ref UCELL_STATS_PATTERN: Regex = Regex::new("PSet0 = \"([^\"]*)\"").unwrap();
    static ref UCELL_CELLS_PATTERN: Regex = Regex::new("PSet = \"([^\"]*)\"").unwrap();
    static ref TCELL_PATTERN: Regex = Regex::new("PSet = \"([^\"]*)\"").unwrap();
}

pub enum Error {
    Unexpected,
    Fetch(anyhow::Error),
}

#[derive(Default)]
pub struct Data {
    pub main: Main,
    pub ucell: Ucell,
    pub tcell: Tcell,
}

#[derive(Default)]
pub struct Main {
    // in mV
    pub voltage: f32,
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
    pub num_slaves: usize,
    pub num_cells: usize,
    pub num_cells_per_slave: usize,
    pub num_temp_sensors: usize,
    pub num_safe_resistors: usize,

    pub overall: VoltageStats,
    pub left: VoltageStats,
    pub right: VoltageStats,

    pub cell_voltage: Vec<u16>,
}

#[derive(Default)]
pub struct VoltageStats {
    // in mV
    pub avg_voltage: u16,
    pub min_voltage: u16,
    pub max_voltage: u16,
    pub delta_voltage: u16,
}

#[derive(Default)]
pub struct Tcell {
    pub overall: TempStats,
    pub left: TempStats,
    pub right: TempStats,

    pub temp: Vec<f32>,
}

#[derive(Default)]
pub struct TempStats {
    pub avg_temp: f32,
    pub min_temp: f32,
    pub max_temp: f32,
    pub delta_temp: f32,
}

pub struct Request {
    main_task: JoinHandle<anyhow::Result<Main>>,
    ucell_task: JoinHandle<anyhow::Result<Ucell>>,
    tcell_task: JoinHandle<anyhow::Result<Tcell>>,
}

pub fn fetch(ip: &str, safe: bool) -> Request {
    let owned_ip = ip.to_string();
    let main_task = thread::spawn(move || main_data(&owned_ip));
    let owned_ip = ip.to_string();
    let ucell_task = thread::spawn(move || ucell(&owned_ip, safe));
    let owned_ip = ip.to_string();
    let tcell_task = thread::spawn(move || tcell(&owned_ip, safe));

    Request {
        main_task,
        ucell_task,
        tcell_task,
    }
}

impl Request {
    pub fn is_finished(&self) -> bool {
        self.main_task.is_finished()
            && self.ucell_task.is_finished()
            && self.tcell_task.is_finished()
    }

    pub fn join(self) -> Result<Data, Error> {
        Ok(Data {
            main: join_task(self.main_task)?,
            ucell: join_task(self.ucell_task)?,
            tcell: join_task(self.tcell_task)?,
        })
    }
}

fn join_task<T>(task: JoinHandle<anyhow::Result<T>>) -> Result<T, Error> {
    match task.join() {
        Ok(Ok(d)) => Ok(d),
        Ok(Err(e)) => Err(Error::Fetch(e)),
        Err(_) => Err(Error::Unexpected),
    }
}

fn main_data(ip: &str) -> anyhow::Result<Main> {
    let url = format!("{ip}/main_data.shtml");
    let resp = ureq::get(&url).call()?;
    let text = resp.into_string()?;

    let stats_captures = MAIN_PATTERN.captures(&text).unwrap();
    let mut stats_iter = stats_captures.get(1).unwrap().as_str().split(',');

    skip(&mut stats_iter, 1);
    let voltage = parse_next::<f32>(&mut stats_iter)? / 1000.0;

    skip(&mut stats_iter, 2);
    let current = parse_next(&mut stats_iter)?;

    skip(&mut stats_iter, 2);
    let state_of_charge = parse_next::<f32>(&mut stats_iter)? / 10.0;

    skip(&mut stats_iter, 2);
    let temp_avg = parse_next::<f32>(&mut stats_iter)? / 10.0;

    skip(&mut stats_iter, 2);
    let temp_min = parse_next::<f32>(&mut stats_iter)? / 10.0;

    skip(&mut stats_iter, 2);
    let temp_max = parse_next::<f32>(&mut stats_iter)? / 10.0;

    skip(&mut stats_iter, 2);
    let temp_master = parse_next::<f32>(&mut stats_iter)? / 10.0;

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

fn ucell(ip: &str, safe: bool) -> anyhow::Result<Ucell> {
    let url = format!("{ip}/ucell.shtml");
    let resp = ureq::get(&url).call()?;
    let text = resp.into_string()?;

    let voltage_captures = UCELL_CELLS_PATTERN.captures(&text).unwrap();
    let mut voltage: Vec<u16> = voltage_captures
        .get(1)
        .unwrap()
        .as_str()
        .split(',')
        .skip(2)
        .map(|s| s.parse::<u16>().unwrap_or(0))
        .collect();

    let avg_voltage = (voltage.iter().map(|n| *n as usize).sum::<usize>() / voltage.len()) as u16;
    if safe {
        for v in &mut voltage {
            if *v < 3000 || *v > 4200 {
                *v = avg_voltage;
            }
        }
    }

    let right = voltage_stats(voltage.iter().take(72).copied());
    let left = voltage_stats(voltage.iter().skip(72).copied());

    let max_voltage = cmp::max(right.max_voltage, left.max_voltage);
    let min_voltage = cmp::min(right.min_voltage, left.min_voltage);
    let overall = VoltageStats {
        avg_voltage,
        max_voltage,
        min_voltage,
        delta_voltage: max_voltage - min_voltage,
    };

    let stats_captures = UCELL_STATS_PATTERN.captures(&text).unwrap();
    let mut stats_iter = stats_captures.get(1).unwrap().as_str().split(',');

    Ok(Ucell {
        num_slaves: parse_next(&mut stats_iter)?,
        num_cells: parse_next(&mut stats_iter)?,
        num_cells_per_slave: parse_next(&mut stats_iter)?,
        num_temp_sensors: parse_next(&mut stats_iter)?,
        num_safe_resistors: parse_next(&mut stats_iter)?,

        overall,
        left,
        right,

        cell_voltage: voltage,
    })
}

fn tcell(ip: &str, safe: bool) -> anyhow::Result<Tcell> {
    let url = format!("{ip}/tcell.shtml");
    let resp = ureq::get(&url).call()?;
    let text = resp.into_string()?;

    let temp_captures = TCELL_PATTERN.captures(&text).unwrap();
    let mut temp: Vec<f32> = temp_captures
        .get(1)
        .unwrap()
        .as_str()
        .split(',')
        .skip(1)
        .map(|s| s.parse::<u16>().unwrap_or(0) as f32 / 10.0)
        .collect();

    let avg_temp = temp.iter().copied().sum::<f32>() / temp.len() as f32;

    let right = temp_stats(temp.iter().take(8).copied());
    let left = temp_stats(temp.iter().skip(8).copied());

    let max_temp = right.max_temp.max(left.max_temp);
    let min_temp = right.min_temp.min(left.min_temp);
    let overall = TempStats {
        avg_temp,
        max_temp,
        min_temp,
        delta_temp: max_temp - min_temp,
    };

    if safe {
        for t in temp.iter_mut() {
            if *t < 15.0 || *t > 45.0 {
                *t = avg_temp;
            }
        }
    }

    Ok(Tcell {
        temp,
        overall,
        left,
        right,
    })
}

fn voltage_stats(voltage: impl Iterator<Item = u16>) -> VoltageStats {
    let mut min = u16::MAX;
    let mut max = 0;
    let mut sum = 0;
    let mut len = 0;
    for v in voltage {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
        sum += v as u32;
        len += 1;
    }
    let delta = max - min;
    let avg = (sum / len) as u16;

    VoltageStats {
        min_voltage: min,
        avg_voltage: avg,
        max_voltage: max,
        delta_voltage: delta,
    }
}

fn temp_stats(voltage: impl Iterator<Item = f32>) -> TempStats {
    let mut min = f32::MAX;
    let mut max = 0.0;
    let mut sum = 0.0;
    let mut len = 0.0;
    for v in voltage {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
        sum += v;
        len += 1.0;
    }
    let delta = max - min;
    let avg = sum / len;

    TempStats {
        min_temp: min,
        avg_temp: avg,
        max_temp: max,
        delta_temp: delta,
    }
}

fn parse_next<T: FromStr>(iter: &mut Split<char>) -> anyhow::Result<T> {
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
