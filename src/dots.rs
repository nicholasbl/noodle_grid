use std::{
    io::BufReader,
    ops::{Add, Div},
    path::Path,
};

/// A single 2D position
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub sx: f64,
    pub sy: f64,
}

/// A pair of positions with a start and end point
#[derive(Debug, Clone, Copy)]
pub struct EndedPosition {
    pub sx: f64,
    pub sy: f64,
    pub ex: f64,
    pub ey: f64,
}

/// A quantity that is split by phase
pub struct Phased<T = f32> {
    pub a: T,
    pub b: T,
    pub c: T,
}

/// A quantity that is split by phase, and is different at start and end points
pub struct EndPhased<T = f32> {
    pub sa: T,
    pub sb: T,
    pub sc: T,
    pub ea: T,
    pub eb: T,
    pub ec: T,
}

impl<T> EndPhased<T>
where
    T: Add<Output = T> + Div<Output = T> + Copy,
    f32: Into<T>,
{
    pub fn average_a(&self) -> T {
        return (self.ea + self.sa) / 2.0.into();
    }

    pub fn average_b(&self) -> T {
        return (self.eb + self.sb) / 2.0.into();
    }

    pub fn average_c(&self) -> T {
        return (self.ec + self.sc) / 2.0.into();
    }
}

/// A timestep record of a line
pub struct LineState {
    pub voltage: EndPhased,
    pub real_power: EndPhased,
    pub reactive_power: EndPhased,
    pub loc: EndedPosition,
}

/// A timestep of a transformer
pub struct TransformerState {
    pub voltage: EndPhased,

    pub tap: Phased<i32>,

    pub tap_changes: Phased<i32>,

    pub loc: Position,
}

/// A timestep of a generator (PV or battery)
pub struct GeneratorState {
    pub voltage: Phased,
    pub angle: Phased,
    pub real: f32,
    pub react: f32,
    pub loc: Position,
}

#[derive(Debug)]
pub struct Floorplan {
    pub ll_x: f64,
    pub ll_y: f64,
    pub ur_x: f64,
    pub ur_y: f64,

    pub data: Vec<u8>,
}

/// A cleaned up dataset
pub struct PowerSystem {
    // These are all states by time;
    // lines[time][line]
    pub title: String,
    pub lines: Vec<Vec<LineState>>,
    pub tfs: Vec<Vec<TransformerState>>,
    pub pvs: Vec<Vec<GeneratorState>>,

    pub floor_plan: Option<Floorplan>,
}

/// Load a power system capnp file
///
/// # Errors
///
/// This function will return an error if the capnp file is incomplete or
/// does not have sufficient timesteps for all elements.
pub fn load_powersystem(path: &Path) -> Result<PowerSystem, anyhow::Error> {
    let file = std::fs::File::open(path)?;
    let buff_reader = BufReader::new(&file);
    let reader = capnp::serialize::read_message(
        buff_reader,
        capnp::message::ReaderOptions {
            traversal_limit_in_words: None,
            ..Default::default()
        },
    )?;

    let ds = reader.get_root::<crate::power_system_capnp::power_system_dataset::Reader>()?;

    let lines = {
        let mut lines = Vec::<Vec<LineState>>::with_capacity(ds.get_lines()?.len() as usize);

        // we are doing a transpose here.
        // The data is coming is as Lines -> Times
        // We want Times -> Lines as thats how we are looking at things

        let line_src = ds.get_lines()?;

        // This is a list of line positions, along with an iterator
        // of the data for that line
        let datas: Vec<_> = line_src
            .iter()
            .map(|f| {
                (
                    EndedPosition {
                        sx: f.get_position_start_x(),
                        sy: f.get_position_start_y(),
                        ex: f.get_position_end_x(),
                        ey: f.get_position_end_y(),
                    },
                    f.get_data().unwrap(),
                    f.get_id().unwrap().to_str().unwrap(),
                    (
                        f.get_voltage_divisor(),
                        f.get_wattage_divisor(),
                        f.get_vars_divisor(),
                    ),
                )
            })
            .collect();

        let mut iters: Vec<_> = datas.iter().map(|f| (f.0, f.1.iter(), f.2, f.3)).collect();

        let time_step_count = line_src.get(0).get_data()?.len();

        log::debug!("Time steps {time_step_count}");

        for _ in 0..time_step_count {
            let mut per_time_step = vec![];

            // for each line...
            for iter in iters.iter_mut() {
                // get the next line state for this line
                let Some(a) = iter.1.next() else {
                    //println!("Line is empty {:?} at {time_step_i}", iter.0);
                    continue;
                };

                let (volt_divisor, watt_divisor, var_divisors) = iter.3;

                per_time_step.push(LineState {
                    voltage: EndPhased {
                        sa: a.get_volt_a_from() / (volt_divisor as f32),
                        sb: a.get_volt_b_from() / (volt_divisor as f32),
                        sc: a.get_volt_c_from() / (volt_divisor as f32),
                        ea: a.get_volt_a_to() / (volt_divisor as f32),
                        eb: a.get_volt_b_to() / (volt_divisor as f32),
                        ec: a.get_volt_c_to() / (volt_divisor as f32),
                    },
                    real_power: EndPhased {
                        sa: a.get_real_a_from() / (watt_divisor as f32),
                        sb: a.get_real_b_from() / (watt_divisor as f32),
                        sc: a.get_real_c_from() / (watt_divisor as f32),
                        ea: a.get_real_a_to() / (watt_divisor as f32),
                        eb: a.get_real_b_to() / (watt_divisor as f32),
                        ec: a.get_real_c_to() / (watt_divisor as f32),
                    },
                    reactive_power: EndPhased {
                        sa: a.get_react_a_from() / (var_divisors as f32),
                        sb: a.get_react_b_from() / (var_divisors as f32),
                        sc: a.get_react_c_from() / (var_divisors as f32),
                        ea: a.get_react_a_to() / (var_divisors as f32),
                        eb: a.get_react_b_to() / (var_divisors as f32),
                        ec: a.get_react_c_to() / (var_divisors as f32),
                    },
                    loc: iter.0,
                });
            }
            lines.push(per_time_step)
        }
        lines
    };

    let tfs = {
        let data_src = ds.get_transformers()?;

        let mut data = Vec::<Vec<TransformerState>>::with_capacity(data_src.len() as usize);

        let datas: Vec<_> = data_src
            .iter()
            .map(|f| {
                (
                    Position {
                        sx: f.get_position_x(),
                        sy: f.get_position_y(),
                    },
                    f.get_data().unwrap(),
                    (
                        f.get_voltage_divisor(),
                        f.get_wattage_divisor(),
                        f.get_vars_divisor(),
                    ),
                )
            })
            .collect();

        let mut iters: Vec<_> = datas.iter().map(|f| (f.0, f.1.iter(), f.2)).collect();

        let time_step_count = data_src.get(0).get_data()?.len();

        for _ in 0..time_step_count {
            let mut per_time_step = vec![];
            for iter in iters.iter_mut() {
                let (volt_divisor, _, _) = iter.2;

                let a = iter.1.next().unwrap();
                per_time_step.push(TransformerState {
                    voltage: EndPhased {
                        sa: a.get_volt_a_from() / (volt_divisor as f32),
                        sb: a.get_volt_b_from() / (volt_divisor as f32),
                        sc: a.get_volt_c_from() / (volt_divisor as f32),
                        ea: a.get_volt_a_to() / (volt_divisor as f32),
                        eb: a.get_volt_b_to() / (volt_divisor as f32),
                        ec: a.get_volt_c_to() / (volt_divisor as f32),
                    },
                    tap: Phased {
                        a: a.get_tap_a(),
                        b: a.get_tap_b(),
                        c: a.get_tap_c(),
                    },
                    tap_changes: Phased {
                        a: a.get_tap_changes_a(),
                        b: a.get_tap_changes_b(),
                        c: a.get_tap_changes_c(),
                    },
                    loc: iter.0,
                });
            }
            data.push(per_time_step)
        }
        data
    };

    let pvs = {
        let data_src = ds.get_generators()?;

        let mut data = Vec::<Vec<GeneratorState>>::with_capacity(data_src.len() as usize);

        let datas: Vec<_> = data_src
            .iter()
            .map(|f| {
                (
                    Position {
                        sx: f.get_position_x(),
                        sy: f.get_position_y(),
                    },
                    f.get_data().unwrap(),
                    (
                        f.get_voltage_divisor(),
                        f.get_wattage_divisor(),
                        f.get_vars_divisor(),
                    ),
                )
            })
            .collect();

        let mut iters: Vec<_> = datas.iter().map(|f| (f.0, f.1.iter(), f.2)).collect();

        let time_step_count = data_src.get(0).get_data()?.len();

        for _ in 0..time_step_count {
            let mut per_time_step = vec![];
            for iter in iters.iter_mut() {
                let (volt_divisor, _, _) = iter.2;

                let a = iter.1.next().unwrap();
                per_time_step.push(GeneratorState {
                    voltage: Phased {
                        a: a.get_volt_a() / (volt_divisor as f32),
                        b: a.get_volt_b() / (volt_divisor as f32),
                        c: a.get_volt_c() / (volt_divisor as f32),
                    },
                    angle: Phased {
                        a: a.get_angle_a(),
                        b: a.get_angle_b(),
                        c: a.get_angle_c(),
                    },
                    real: a.get_real(),
                    react: a.get_react(),
                    loc: iter.0,
                });
            }
            data.push(per_time_step)
        }
        data
    };

    let title = figure_name(path);

    let fp: Option<Floorplan> = if let Ok(fp) = ds.get_floorplan() {
        use crate::power_system_capnp::floor_plan::Which;

        match fp.which().unwrap() {
            Which::ImageEmbedded(Ok(x)) => Some(Floorplan {
                ll_x: fp.get_lower_left_x(),
                ll_y: fp.get_lower_left_y(),
                ur_x: fp.get_upper_right_x(),
                ur_y: fp.get_upper_right_y(),
                data: x.to_owned(),
            }),
            Which::ImageURL(_) => unimplemented!("Fetch yet implemented"),
            _ => None,
        }
    } else {
        None
    };

    Ok(PowerSystem {
        title,
        lines,
        tfs,
        pvs,
        floor_plan: fp,
    })
}

/// Attempt to determine the dataset name. If unable, the filename will be used.
fn figure_name(path: &Path) -> String {
    let res = extract_name(path);

    if let Some(name) = res {
        return name;
    }

    path.file_stem()
        .and_then(|f| f.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// Inspect the capnp file for a title
fn extract_name(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let buff_reader = BufReader::new(&file);
    let reader = capnp::serialize::read_message(
        buff_reader,
        capnp::message::ReaderOptions {
            traversal_limit_in_words: None,
            ..Default::default()
        },
    )
    .ok()?;

    let ds = reader
        .get_root::<crate::power_system_capnp::power_system_dataset::Reader>()
        .ok()?;

    let string = ds.get_name().ok()?.to_string().ok()?;

    if string.is_empty() {
        return None;
    }

    Some(string)
}
