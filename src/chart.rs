use image::ExtendedColorType;
use itertools::Itertools;
use plotters::prelude::*;

use crate::PowerSystem;

pub fn generate_time_chart(system: &PowerSystem, width: u32, height: u32) -> Vec<u8> {
    let mut buff = vec![0; (width * height * 3) as usize];

    {
        let root = BitMapBackend::with_buffer(&mut buff, (width, height)).into_drawing_area();

        root.fill(&WHITE).unwrap();

        // lines are [time][line_i]

        let line_count = system.lines.first().map(|l| l.len()).unwrap_or(1);
        let time_count = system.lines.len();

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .caption(format!("{}: Details", system.title), ("sans-serif", 40))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Right, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 60)
            .build_cartesian_2d(0..time_count, 0.5..1.5)
            .unwrap();

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(15)
            .max_light_lines(4)
            .x_label_style(("arial", 24))
            .y_label_style(("arial", 24))
            .x_desc("Sample")
            .y_desc("volts")
            .draw()
            .unwrap();

        for line_i in 0..line_count {
            let data: Vec<_> = system.lines.iter().map(|l| l[line_i].voltage.ea).collect();

            chart
                .draw_series(LineSeries::new(
                    data.iter()
                        .enumerate()
                        .map(|(time, &value)| (time, value as f64)),
                    &RGBColor(120, 120, 255),
                ))
                .unwrap();
        }

        root.present().unwrap();
    }

    buffer_to_png(&buff, width, height)
}

fn buffer_to_png(source: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut png_buffer = std::io::Cursor::new(Vec::<u8>::new());

    image::write_buffer_with_format(
        &mut png_buffer,
        source,
        width,
        height,
        ExtendedColorType::Rgb8,
        image::ImageFormat::Png,
    )
    .unwrap();

    png_buffer.into_inner()
}

pub fn generate_chart_for(line_i: usize, system: &PowerSystem) -> Vec<u8> {
    let data_power: Vec<_> = system
        .lines
        .iter()
        //.map(|l| l[line_i].voltage.average_a())
        .map(|l| l[line_i].real_power.average())
        .collect();

    let data_voltage: Vec<_> = system.lines.iter().map(|l| l[line_i].voltage.ea).collect();

    let power_minmax = match data_power.iter().minmax() {
        itertools::MinMaxResult::MinMax(&a, &b) => (a, b),
        _ => (0.0, 1.0),
    };

    let voltage_minmax = match data_voltage.iter().minmax() {
        itertools::MinMaxResult::MinMax(&a, &b) => (a, b),
        _ => (0.0, 1.0),
    };

    let size = (1024u32, 768u32);
    let mut buff = vec![0; (size.0 * size.1 * 3) as usize];

    let name = &system.line_meta[line_i];

    {
        let root = BitMapBackend::with_buffer(&mut buff, size).into_drawing_area();

        root.fill(&WHITE).unwrap();

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .caption(format!("{name}: Details"), ("sans-serif", 40))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Right, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(0..data_power.len(), power_minmax.0..power_minmax.1)
            .unwrap()
            .set_secondary_coord(0..data_voltage.len(), voltage_minmax.0..voltage_minmax.1);

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(30)
            .max_light_lines(4)
            .y_desc("kW")
            .draw()
            .unwrap();

        chart
            .configure_secondary_axes()
            .y_desc("volts")
            .draw()
            .unwrap();

        chart
            .draw_series(LineSeries::new(
                data_power
                    .iter()
                    .enumerate()
                    .map(|(time, &value)| (time, value)),
                &BLUE,
            ))
            .unwrap();

        chart
            .draw_secondary_series(LineSeries::new(
                data_voltage
                    .iter()
                    .enumerate()
                    .map(|(time, &value)| (time, value)),
                &RED,
            ))
            .unwrap();

        root.present().unwrap();
    }

    buffer_to_png(&buff, size.0, size.1)
}
