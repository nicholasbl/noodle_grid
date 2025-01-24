use colabrodo_common::common::strings;
use colabrodo_server::{server::*, server_messages::*};
use nalgebra_glm as glm;
use nalgebra_glm::scaling;
use nalgebra_glm::Vec3;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::probe::update_probes;
use crate::probe::Probe;
use crate::state::*;

make_method_function!(set_time,
GridState,
"noo::set_time",
"Set the time of the visualization",
| time : Value : "Floating point time" |,
{
    let time : f32 = from_cbor(time).unwrap_or_default();
    let time : usize = time as usize;
    let time = time.clamp(0, app.max_time_step - 1);
    app.time_step = time;
    recompute_all(app, state);
    Ok(None)
});

// =============================================================================

make_method_function!(step_time,
GridState,
"noo::step_time",
"Advance the time of the visualization",
| time : Value : "Integer step direction" |,
{
    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = (app.time_step as i32 + time).clamp(0, app.max_time_step as i32 - 1);

    log::debug!("Stepping time: {time}");

    app.time_step = time as usize;
    recompute_all(app, state);

    log::debug!("All done");
    Ok(None)
});

async fn advance_timer(
    send_back: tokio::sync::mpsc::Sender<bool>,
    mut to_stop: tokio::sync::oneshot::Receiver<bool>,
) {
    loop {
        log::debug!("Advancer");
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs_f32(0.25)) => {
                log::debug!("Sleep done");
                if send_back.send(true).await.is_err() {
                    log::debug!("closing advance timer");
                    return
                }
            },
            _ = &mut to_stop => {
                log::debug!("closing advance timer");
                return
            }
        }
    }
}

fn check_launch_timer(gs: &mut GridState, start_timer: bool) {
    if gs.active_timer.is_some() {
        if start_timer {
            // already have timer going. skip
        } else {
            // timer going and we want to stop. issue stop.
            log::debug!("Issuing stop");
            let sender = gs.active_timer.take().unwrap();
            let _ = sender.send(true);
        }
    } else if start_timer {
        // timer is not running and they want one. start;
        log::debug!("Launching player");
        let (os_tx, os_rx) = tokio::sync::oneshot::channel();

        gs.active_timer = Some(os_tx);
        let send_back = gs.send_back.clone().unwrap();

        tokio::spawn(advance_timer(send_back, os_rx));
    } else {
        // timer not running and they want a stop. skip
    }
    log::debug!("Check launch done");
}

make_method_function!(play_time,
GridState,
"noo::animate_time",
"Play the visualization",
| time : Value : "Integer step direction" |,
{
    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = time.clamp(0, 1) == 1;

    log::debug!("Asking to play time: {time}");

    check_launch_timer(app, time);
    Ok(None)
});

pub async fn advance_watcher(gs: GridStatePtr, mut rx: tokio::sync::mpsc::Receiver<bool>) {
    while rx.recv().await.is_some() {
        log::debug!("advancing time");
        let mut lock = gs.lock().unwrap();
        lock.time_step = (lock.time_step + 1) % lock.max_time_step;

        let ss_arc = lock.state.clone();
        let mut ss_lock = ss_arc.lock().unwrap();

        recompute_all(&mut lock, &mut ss_lock);
    }
}

// =============================================================================

fn make_probe(gs: &mut GridState, state: &mut ServerState, context: Option<InvokeIDType>) {
    if let Some(_context) = context {
        // Will do some more stuff here
    } else {
        // at the moment, this is a global create probe
        // max 5
        if gs.probes.len() >= 5 {
            gs.probes.pop_front();
        }

        const TEX_CUBE: &str = include_str!("../assets/probe_icon.obj");

        let contents = std::io::BufReader::new(std::io::Cursor::new(TEX_CUBE));

        let hazard_mat = state.materials.new_component(ServerMaterialState {
            name: None,
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [0.5, 0.5, 1.0, 1.0],
                    metallic: Some(0.0),
                    roughness: Some(1.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        });

        let (ent, _) = crate::import_obj::import_file(
            contents,
            state,
            Some(scaling(&Vec3::repeat(0.25))),
            Some(hazard_mat),
        )
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

        let update = ServerEntityStateUpdatable {
            methods_list: Some(vec![gs.move_func.clone().unwrap()]),
            ..Default::default()
        };

        update.patch(&ent);

        gs.probes.push_back(Probe::new(ent));

        update_probes(gs, state);
    }
}

make_method_function!(
    create_probe,
    GridState,
    "create_probe",
    "Add a probe to the visualization",
    {
        make_probe(app, state, None);
        Ok(None)
    }
);

make_method_function!(item_activate, GridState, "noo::activate", "Activate", {
    make_probe(app, state, context);
    Ok(None)
});

// =============================================================================

fn on_move(
    gs: &mut GridState,
    state: &mut ServerState,
    context: Option<InvokeIDType>,
    position: [f32; 3],
) {
    let Some(ctx) = context else {
        return;
    };

    let ctx = match ctx {
        InvokeIDType::Entity(e) => e,
        _ => {
            return;
        }
    };

    let Some(ctx) = state.entities.resolve(ctx) else {
        return;
    };

    // For now...

    let mut probe_changed = false;

    for item in &mut gs.probes {
        if item.entity != ctx {
            continue;
        }

        item.dirty = true;

        let mut new_p: Vec3 = position.into();

        new_p.y = 0.0;

        item.world_pos = new_p;

        probe_changed = true;
    }

    if probe_changed {
        update_probes(gs, state);
        return;
    }

    // not a probe. just set the position they want

    let placement: [f32; 16] = {
        let spot: Vec3 = position.into();
        let tf = glm::translation(&spot);
        tf.as_slice().try_into().unwrap()
    };

    let update = ServerEntityStateUpdatable {
        transform: Some(placement),
        ..Default::default()
    };

    update.patch(&ctx);
}

make_method_function!(set_position,
    GridState,
    strings::MTHD_SET_POSITION,
    "Set the position of an entity.",
    |position : [f32;3] : "New position of entity, as vec3"|,
    {
        on_move(app, state, context, position);
        Ok(None)
    }
);

// =============================================================================

make_method_function!(
    toggle_line_load,
    GridState,
    "toggle_line_load",
    "Toggle visibility of line loading",
    { Ok(None) }
);
