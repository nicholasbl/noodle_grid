mod arguments;
mod dots;
mod power_system_capnp;
mod state;

use state::*;

use arguments::Arguments;
use clap::Parser;
use colabrodo_server::server::*;

use dots::*;

#[tokio::main]
async fn main() {
    env_logger::init();

    println!("Connect clients to localhost:50000");

    let opts = ServerOptions::default();

    let state = ServerState::new();

    let app_state = GridState::new(state.clone(), load_data());

    {
        let mut lock = app_state.lock().unwrap();
        lock.recompute_all();
    }

    let mdns = mdns_publish(opts.host.port().unwrap());

    server_main(opts, state).await;

    mdns.shutdown().unwrap();
}

fn load_data() -> PowerSystem {
    let args = Arguments::parse();

    load_powersystem(&args.pack_path).expect("loading powersystem")
}

fn mdns_publish(port: u16) -> mdns_sd::ServiceDaemon {
    let mdns = mdns_sd::ServiceDaemon::new().expect("unable to create mdns daemon");

    const SERVICE_TYPE: &str = "_noodles._tcp.local.";
    const INSTANCE_NAME: &str = "noodle_grid";

    if let Ok(nif) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in nif.iter().filter(|f| f.1.is_ipv4()) {
            let ip_str = ip.to_string();
            let host = format!("{}.local.", ip);

            let srv_info =
                mdns_sd::ServiceInfo::new(SERVICE_TYPE, INSTANCE_NAME, &host, ip_str, port, None)
                    .expect("unable to  build MDNS service information");

            log::info!("registering MDNS SD on {}", ip);

            if mdns.register(srv_info).is_err() {
                log::warn!("unable to register MDNS SD for {}", ip);
            }
        }
    }

    mdns
}
