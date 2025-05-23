use colabrodo_common::common::strings;
use colabrodo_server::{server::*, server_messages::*};
use nalgebra_glm as glm;
use nalgebra_glm::scaling;
use nalgebra_glm::Vec3;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::probe::update_probes;
use crate::probe::ClickResult;
use crate::probe::Probe;
use crate::state::*;

make_method_function!(set_time,
GridState,
"noo::set_time",
"Set the time of the visualization",
| time : Value : "Floating point time" |,
{
    //! Sets the current time step based on a floating-point input.
    //!
    //! Clamps the input to valid range and triggers recomputation.
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
    //! Steps the current time step by a signed integer amount.
    //!
    //! Used for manual time navigation forward/backward.

    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = (app.time_step as i32 + time).clamp(0, app.max_time_step as i32 - 1);

    log::debug!("Stepping time: {time}");

    app.time_step = time as usize;
    recompute_all(app, state);

    log::debug!("All done");
    Ok(None)
});

/// Periodically signals a timer channel until cancelled.
///
/// Runs in a background task to drive automatic time advancement.
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

/// Starts or stops the playback timer based on the requested direction.
///
/// Manages an internal oneshot channel to control the timer lifecycle.
fn check_launch_timer(gs: &mut GridState, timer_direction: i32) {
    let start_timer = timer_direction != 0;
    gs.time_step_direction = timer_direction;

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
    //! Starts or stops automatic time playback based on a step direction.
    //!
    //! -1: reverse, 0: stop, 1: forward

    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = time.clamp(-1, 1);

    log::debug!("Asking to play time: {time}");

    check_launch_timer(app, time);
    Ok(None)
});

/// Watches for timer signals and advances the visualization time step.
///
/// Recomputes the entire scene each time the step updates.
pub async fn advance_watcher(gs: GridStatePtr, mut rx: tokio::sync::mpsc::Receiver<bool>) {
    while rx.recv().await.is_some() {
        log::debug!("advancing time");
        let mut lock = gs.lock().unwrap();

        let mut new_time =
            (lock.time_step as i32 + lock.time_step_direction) % lock.max_time_step as i32;

        // do a wrapping sub here
        if new_time < 0 {
            new_time += lock.max_time_step as i32
        }

        lock.time_step = new_time.try_into().unwrap();

        let ss_arc = lock.state.clone();
        let mut ss_lock = ss_arc.lock().unwrap();

        {
            let time_frac = lock.time_frac();
            lock.summary.set_time_normalized(time_frac);
            //lock.summary.set_time_normalized(0.0);
        }

        recompute_all(&mut lock, &mut ss_lock);
    }
}

// =============================================================================

/// Creates a new probe entity in the scene (if under probe limit).
///
/// Probes are lightweight movable entities users can interact with.
fn make_probe(gs: &mut GridState, state: &mut ServerState, context: Option<InvokeIDType>) {
    if let Some(_context) = context {
        // Will do some more stuff here
    } else {
        // Limit the number of probes to 5 by recycling oldest
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

        // Import probe mesh from embedded OBJ asset
        let (ent, _) = crate::import_obj::import_file(
            contents,
            state,
            Some(scaling(&Vec3::repeat(0.25))),
            None,
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

        gs.probe_move_request_signal.send(true).unwrap();
    }
}

make_method_function!(
    create_probe,
    GridState,
    "Create Probe",
    "Add a probe to the visualization",
    {
        //! User-invoked method to create a new probe.
        make_probe(app, state, None);
        Ok(None)
    }
);

make_method_function!(
    item_activate,
    GridState,
    "noo::activate",
    "Activate",
    {
        //! Handles activation of an item, which can trigger probe creation.
        make_probe(app, state, context);
        Ok(None)
    }
);

// =============================================================================

/// Moves an entity or probe to a new position based on a remote input.
///
/// If the entity is a known probe, updates internal tracking.
fn on_move(
    gs: &mut GridState,
    state: &mut ServerState,
    context: Option<InvokeIDType>,
    position: [f32; 3],
) {
    // Has to be invoked on an entity
    let Some(InvokeIDType::Entity(ctx)) = context else {
        return;
    };

    // And we have to know about it
    let Some(ctx) = state.entities.resolve(ctx) else {
        return;
    };

    // See if any of the probes have changed
    for item in &mut gs.probes {
        // Check if the moved entity matches a probe
        if item.entity != ctx {
            continue;
        }

        log::debug!("Sending move update");
        gs.probe_move_request_signal.send(true).unwrap();

        // This entity is a probe we are changing

        let mut new_p: Vec3 = position.into();

        new_p.y = 0.0;

        item.dirty = Some(new_p);

        log::debug!("Done with move update");

        return;
    }

    // Otherwise treat as a generic move and update transform directly
    let placement: [f32; 16] = {
        let spot: Vec3 = position.into();
        let tf = glm::translation(&spot);
        tf.as_slice().try_into().unwrap()
    };

    let update = ServerEntityStateUpdatable {
        transform: Some(placement),
        ..Default::default()
    };

    // Patch what changed
    update.patch(&ctx);
}

/// Background task that watches for probe movement signals.
///
/// When triggered, updates probe transforms accordingly.
pub async fn probe_service(
    gs: GridStatePtr,
    mut check: tokio::sync::mpsc::UnboundedReceiver<bool>,
) {
    while check.recv().await.is_some() {
        log::debug!("Getting move update");

        log::debug!("Proceeding...");

        update_probes(gs.clone());
    }
}

make_method_function!(set_position,
    GridState,
    strings::MTHD_SET_POSITION,
    "Set the position of an entity.",
    |position : [f32;3] : "New position of entity, as vec3"|,
    {
        //! Method to update an entity's world position.
        on_move(app, state, context, position);
        Ok(None)
    }
);

// =============================================================================

make_method_function!(
    toggle_line_load,
    GridState,
    "Toggle Line Load",
    "Toggle visibility of line loading",
    {
        //! Toggles between normal line coloring and line load visualization.

        app.show_line_load = !app.show_line_load;
        recompute_all(app, state);
        Ok(None)
    }
);

// =============================================================================

/// Handles click events on entities, possibly deleting a probe.
///
/// A click can trigger a probe "check_click" event to self-remove.
fn on_click(
    gs: &mut GridState,
    state: &mut ServerState,
    context: Option<InvokeIDType>,
    _ty: Option<ciborium::Value>,
) {
    // Has to be invoked on an entity
    let Some(InvokeIDType::Entity(ctx)) = context else {
        return;
    };

    // And we have to know about it
    let Some(ctx) = state.entities.resolve(ctx) else {
        return;
    };

    gs.probes
        .retain_mut(|f| !matches!(f.check_click(&ctx), Some(ClickResult::Delete)));
}

make_method_function!(activate,
    GridState,
    strings::MTHD_ACTIVATE,
    "Activate an entity",
    | kind : Option<ciborium::Value> : "Activation context"|,
    {
        //! Invokes an activation action on an entity (usually a probe or hazard).

        on_click(app, state, context, kind);
        Ok(None)
    }
);
