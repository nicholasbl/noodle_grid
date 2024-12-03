mod arguments;
mod basemap;
mod chart;
mod domain;
mod dots;
mod geometry;
mod hazard;
mod import_obj;
mod instance;
mod instanced_item;
mod methods;
mod power_system_capnp;
mod probe;
mod ruler;
mod state;
mod texture;
mod utility;

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
        let ip_list: Vec<_> = nif
            .iter()
            .filter_map(|f| match f.1 {
                std::net::IpAddr::V4(ipv4_addr) => Some(ipv4_addr),
                _ => None,
            })
            .filter_map(|f| {
                let str = f.to_string();
                if str != "127.0.0.1" {
                    Some(str)
                } else {
                    None
                }
            })
            .collect();

        let hname = hostname::get().unwrap_or_else(|_| ip_list.first().unwrap().into());

        let hname = hname.into_string().unwrap();

        let host = format!("{}.local.", hname);

        let srv_info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host,
            ip_list.as_slice(),
            port,
            None,
        )
        .expect("unable to build MDNS service information");

        log::info!("registering MDNS SD on {name} {ip_list:?}");

        if mdns.register(srv_info).is_err() {
            log::warn!("unable to register MDNS!");
        }
    }

    mdns
}
