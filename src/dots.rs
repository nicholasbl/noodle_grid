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
    T: Add<Output = T> + Div<Output = T> + Copy + std::iter::Sum,
    f32: Into<T>,
{
    pub fn scaled(self, scale: T) -> Self {
        Self {
            sa: self.sa / scale,
            sb: self.sb / scale,
            sc: self.sc / scale,
            ea: self.ea / scale,
            eb: self.eb / scale,
            ec: self.ec / scale,
        }
    }

    pub fn average(&self) -> T {
        let sum: T = [self.sa, self.sb, self.sc, self.ea, self.eb, self.ec]
            .into_iter()
            .sum();
        sum / 6.0.into()
    }
}

/// A timestep record of a line
pub struct LineState {
    pub voltage: EndPhased,
    pub real_power: EndPhased,
    pub reactive_power: EndPhased,
    pub loc: EndedPosition,
    pub line_load: Phased<f32>,
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

/// Options for a map to show power systems context
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

    pub line_meta: Vec<String>,

    pub floor_plan: Option<Floorplan>,
}

/// Loads a `PowerSystem` from a Cap'n Proto file on disk.
///
/// # Errors
///
/// This function will return an error if the capnp file is incomplete or
/// does not have sufficient timesteps for all elements.
pub fn load_powersystem(path: &Path) -> Result<PowerSystem, anyhow::Error> {
    // Open the file and deserialize the Cap'n Proto message
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

    // Load components individually
    let lines = load_lines(&ds)?;
    let tfs = load_transformers(&ds)?;
    let pvs = load_generators(&ds)?;
    let title = figure_name(path);
    let line_meta = load_line_metadata(&ds);
    let floor_plan = load_floorplan(&ds);

    // Assemble final PowerSystem
    Ok(PowerSystem {
        title,
        lines,
        tfs,
        pvs,
        floor_plan,
        line_meta,
    })
}

/// Loads line data, transposing it from (Lines -> Times) into (Times -> Lines).
fn load_lines(
    ds: &crate::power_system_capnp::power_system_dataset::Reader,
) -> Result<Vec<Vec<LineState>>, anyhow::Error> {
    let line_src = ds.get_lines()?;
    let mut lines = Vec::with_capacity(line_src.len() as usize);

    // Collect static info and iterators
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

    // Build per-time-step slices
    for _ in 0..time_step_count {
        let mut per_time_step = vec![];

        for iter in iters.iter_mut() {
            let Some(a) = iter.1.next() else {
                continue;
            };
            let (volt_div, watt_div, var_div) = iter.3;

            per_time_step.push(LineState {
                voltage: EndPhased {
                    sa: a.get_volt_a_from(),
                    sb: a.get_volt_b_from(),
                    sc: a.get_volt_c_from(),
                    ea: a.get_volt_a_to(),
                    eb: a.get_volt_b_to(),
                    ec: a.get_volt_c_to(),
                }
                .scaled(volt_div as f32),
                real_power: EndPhased {
                    sa: a.get_real_a_from(),
                    sb: a.get_real_b_from(),
                    sc: a.get_real_c_from(),
                    ea: a.get_real_a_to(),
                    eb: a.get_real_b_to(),
                    ec: a.get_real_c_to(),
                }
                .scaled(watt_div as f32),
                reactive_power: EndPhased {
                    sa: a.get_react_a_from(),
                    sb: a.get_react_b_from(),
                    sc: a.get_react_c_from(),
                    ea: a.get_react_a_to(),
                    eb: a.get_react_b_to(),
                    ec: a.get_react_c_to(),
                }
                .scaled(var_div as f32),
                line_load: Phased {
                    a: a.get_line_load_real_a(),
                    b: a.get_line_load_real_b(),
                    c: a.get_line_load_real_c(),
                },
                loc: iter.0,
            });
        }
        lines.push(per_time_step);
    }

    Ok(lines)
}

/// Loads transformer data, organized by time step.
fn load_transformers(
    ds: &crate::power_system_capnp::power_system_dataset::Reader,
) -> Result<Vec<Vec<TransformerState>>, anyhow::Error> {
    let data_src = ds.get_transformers()?;
    let mut transformers = Vec::with_capacity(data_src.len() as usize);

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
            let (volt_div, _, _) = iter.2;
            let a = iter.1.next().unwrap();

            per_time_step.push(TransformerState {
                voltage: EndPhased {
                    sa: a.get_volt_a_from() / (volt_div as f32),
                    sb: a.get_volt_b_from() / (volt_div as f32),
                    sc: a.get_volt_c_from() / (volt_div as f32),
                    ea: a.get_volt_a_to() / (volt_div as f32),
                    eb: a.get_volt_b_to() / (volt_div as f32),
                    ec: a.get_volt_c_to() / (volt_div as f32),
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
        transformers.push(per_time_step);
    }

    Ok(transformers)
}

/// Loads generator (PV) data, organized by time step.
fn load_generators(
    ds: &crate::power_system_capnp::power_system_dataset::Reader,
) -> Result<Vec<Vec<GeneratorState>>, anyhow::Error> {
    let data_src = ds.get_generators()?;
    let mut generators = Vec::with_capacity(data_src.len() as usize);

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
            let (volt_div, _, _) = iter.2;
            let a = iter.1.next().unwrap();

            per_time_step.push(GeneratorState {
                voltage: Phased {
                    a: a.get_volt_a() / (volt_div as f32),
                    b: a.get_volt_b() / (volt_div as f32),
                    c: a.get_volt_c() / (volt_div as f32),
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
        generators.push(per_time_step);
    }

    Ok(generators)
}

/// Loads line names metadata, falling back to "Unknown" if missing.
fn load_line_metadata(ds: &crate::power_system_capnp::power_system_dataset::Reader) -> Vec<String> {
    ds.get_lines()
        .unwrap()
        .iter()
        .map(|f| f.get_name().ok().and_then(|r| r.to_string().ok()))
        .map(|f| f.unwrap_or_else(|| "Unknown".into()))
        .collect()
}

/// Attempts to load an embedded floorplan image (optional).
fn load_floorplan(
    ds: &crate::power_system_capnp::power_system_dataset::Reader,
) -> Option<Floorplan> {
    if let Ok(fp) = ds.get_floorplan() {
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
    }
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
