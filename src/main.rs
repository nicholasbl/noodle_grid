mod arguments;
mod dots;
mod geometry;
mod power_system_capnp;
mod state;
mod texture;

use state::*;

use arguments::Arguments;
use clap::Parser;
use colabrodo_server::server::*;

use dots::*;

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("Connect clients to port 50000");

    let opts = ServerOptions::default();

    let state = ServerState::new();

    let data = load_data();
    let data_title = data.title.clone();
    log::info!("Loaded dataset: {data_title}");

    let app_state = GridState::new(state.clone(), data);

    GridState::post_setup(&state, &app_state);

    {
        let mut lock = app_state.lock().unwrap();
        recompute_all(&mut lock, &mut state.lock().unwrap());
    }

    let mdns = mdns_publish(opts.host.port().unwrap(), data_title);

    server_main(opts, state).await;

    mdns.shutdown().unwrap();
}

fn load_data() -> PowerSystem {
    let args = Arguments::parse();

    load_powersystem(&args.pack_path).expect("loading powersystem")
}

fn mdns_publish(port: u16, name: String) -> mdns_sd::ServiceDaemon {
    let mdns = mdns_sd::ServiceDaemon::new().expect("unable to create mdns daemon");

    const SERVICE_TYPE: &str = "_noodles._tcp.local.";

    let instance_name = format!("grid: {name}");

    if let Ok(nif) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in nif.iter().filter(|f| f.1.is_ipv4()) {
            let ip_str = ip.to_string();
            let host = format!("{}.local.", ip);

            let srv_info =
                mdns_sd::ServiceInfo::new(SERVICE_TYPE, &instance_name, &host, ip_str, port, None)
                    .expect("unable to build MDNS service information");

            log::info!("registering MDNS SD on {}", ip);

            if mdns.register(srv_info).is_err() {
                log::warn!("unable to register MDNS SD for {}", ip);
            }
        }
    }

    mdns
}
