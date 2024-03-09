use std::{io::BufReader, path::Path};

// enum End {
//     A,
//     B,
// }

// enum Phase {
//     A,
//     B,
//     C,
// }

#[derive(Debug, Clone, Copy)]
pub struct EndedPosition {
    pub sx: f64,
    pub sy: f64,
    pub ex: f64,
    pub ey: f64,
}

pub struct Phased {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

pub struct EndPhased {
    pub sa: f32,
    pub sb: f32,
    pub sc: f32,
    pub ea: f32,
    pub eb: f32,
    pub ec: f32,
}

pub struct LineState {
    pub voltage: EndPhased,
    pub real_power: EndPhased,
    pub reactive_power: EndPhased,
    pub loc: EndedPosition,
}

pub struct TransformerState {
    pub voltage: EndPhased,

    pub volt_a_to: f32,
    pub volt_b_to: f32,
    pub volt_c_to: f32,

    pub tap_a: i32,
    pub tap_b: i32,
    pub tap_c: i32,

    pub tap_changes_a: i32,
    pub tap_changes_b: i32,
    pub tap_changes_c: i32,
}

pub struct GeneratorState {
    pub voltage: Phased,
    pub angle: Phased,
    pub real: f32,
    pub react: f32,
}

pub struct PowerSystem {
    pub lines: Vec<Vec<LineState>>,
    pub tfs: Vec<Vec<TransformerState>>,
    pub pvs: Vec<Vec<GeneratorState>>,
}

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

        // we are doing a transpose here

        let line_src = ds.get_lines()?;

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
                )
            })
            .collect();

        let mut iters: Vec<_> = datas.iter().map(|f| (f.0, f.1.iter())).collect();

        let time_step_count = line_src.get(0).get_data()?.len();

        for _ in 0..time_step_count {
            let mut per_time_step = vec![];
            for iter in iters.iter_mut() {
                let a = iter.1.next().unwrap();
                per_time_step.push(LineState {
                    voltage: EndPhased {
                        sa: a.get_volt_a_from(),
                        sb: a.get_volt_b_from(),
                        sc: a.get_volt_c_from(),
                        ea: a.get_volt_a_to(),
                        eb: a.get_volt_b_to(),
                        ec: a.get_volt_c_to(),
                    },
                    real_power: EndPhased {
                        sa: a.get_real_a_from(),
                        sb: a.get_real_b_from(),
                        sc: a.get_real_c_from(),
                        ea: a.get_real_a_to(),
                        eb: a.get_real_b_to(),
                        ec: a.get_real_c_to(),
                    },
                    reactive_power: EndPhased {
                        sa: a.get_react_a_from(),
                        sb: a.get_react_b_from(),
                        sc: a.get_react_c_from(),
                        ea: a.get_react_a_to(),
                        eb: a.get_react_b_to(),
                        ec: a.get_react_c_to(),
                    },
                    loc: iter.0,
                });
            }
            lines.push(per_time_step)
        }
        lines
    };

    Ok(PowerSystem {
        lines,
        tfs: vec![],
        pvs: vec![],
    })

    // tfs: Vec<Vec<TransformerState>>,
    // pvs: Vec<Vec<GeneratorState>>,
}
