use image::ExtendedColorType;
use itertools::Itertools;
use plotters::prelude::*;

use crate::PowerSystem;

//TODO: We get a stutter when people are smoothly moving things around...
// user is dragging. update for position comes in. we process it, and send back the position update to all users including the dragger. now if the user drags and then lets go, new updates keep coming in. so we need request rejection to stop processing updates for certain probes, and only keep the last

pub fn generate_chart_for(line_i: usize, system: &PowerSystem) -> Vec<u8> {
    let data: Vec<_> = system
        .lines
        .iter()
        //.map(|l| l[line_i].voltage.average_a())
        .map(|l| l[line_i].real_power.average_a())
        .collect();

    let minmax = match data.iter().minmax() {
        itertools::MinMaxResult::MinMax(&a, &b) => (a, b),
        _ => (0.0, 1.0),
    };

    let size = (1024u32, 768u32);
    let mut buff = vec![0; (size.0 * size.1 * 3) as usize];

    {
        let root = BitMapBackend::with_buffer(&mut buff, size).into_drawing_area();

        root.fill(&WHITE).unwrap();

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .caption("Power", ("sans-serif", 40))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Right, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(0..data.len(), minmax.0..minmax.1)
            .unwrap();

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(30)
            .max_light_lines(4)
            .y_desc("watts")
            .draw()
            .unwrap();

        chart
            .draw_series(LineSeries::new(
                data.iter().enumerate().map(|(time, &value)| (time, value)),
                &BLUE,
            ))
            .unwrap();

        root.present().unwrap();
    }

    let mut png_buffer = std::io::Cursor::new(Vec::<u8>::new());

    image::write_buffer_with_format(
        &mut png_buffer,
        &buff,
        size.0,
        size.1,
        ExtendedColorType::Rgb8,
        image::ImageFormat::Png,
    )
    .unwrap();

    png_buffer.into_inner()
}
