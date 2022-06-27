use anyhow::{anyhow, Result};
use clap::Parser;
use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use lazy_static::lazy_static;
use podman_api::opts::ContainerListOpts;
use podman_api::Podman;
use prometheus::{register_gauge, register_gauge_vec, Encoder, Gauge, GaugeVec, TextEncoder};
use serde_json::Value;
use std::collections::hash_map::HashMap;
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, Parser)]
struct AppArgs {
    #[clap(short, long, default_value = "127.0.0.1")]
    host: String,
    #[clap(short, long, default_value = "9807")]
    port: u16,
    #[clap(long, default_value = "unix:///run/podman/podman.sock")]
    podman: String,
}

lazy_static! {
    static ref ARGS: AppArgs = AppArgs::parse();
    static ref COLLECTOR: Collector = Collector::new(&ARGS.podman).unwrap();
    static ref CONTAINER_TOTAL: Gauge =
        register_gauge!("podman_container_total", "Total count of containers").unwrap();
    static ref CONTAINER_COUNT: GaugeVec =
        register_gauge_vec!("podman_container_count", "Count of containers", &["pod"],).unwrap();
    static ref CONTAINER_STATE: GaugeVec = register_gauge_vec!(
        "podman_container_state",
        "Container current state (-1=unknown,0=exited/stopped,1=running,2=created)",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_UPTIME: GaugeVec = register_gauge_vec!(
        "podman_container_uptime",
        "Container uptime",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_SYSTEM_NANO: GaugeVec = register_gauge_vec!(
        "podman_container_system_nano",
        "Container system nano",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_PIDS: GaugeVec = register_gauge_vec!(
        "podman_container_pids",
        "Count of running pids in container",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_AVG_CPU: GaugeVec = register_gauge_vec!(
        "podman_container_avg_cpu",
        "Container Avg CPU usage",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_CPU: GaugeVec = register_gauge_vec!(
        "podman_container_cpu",
        "Container CPU usage",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_CPU_NANO: GaugeVec = register_gauge_vec!(
        "podman_container_cpu_nano",
        "Container CPU usage (nano)",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_CPU_SYSTEM_NANO: GaugeVec = register_gauge_vec!(
        "podman_container_cpu_system_nano",
        "Container CPU usage (system nano)",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_MEM_USAGE: GaugeVec = register_gauge_vec!(
        "podman_container_mem_usage",
        "Container memory usage (bytes)",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_MEM_LIMIT: GaugeVec = register_gauge_vec!(
        "podman_container_mem_limit",
        "Container memory limit",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_MEM_PERC: GaugeVec = register_gauge_vec!(
        "podman_container_mem_perc",
        "Container memory usage (percentage)",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_NET_INP: GaugeVec = register_gauge_vec!(
        "podman_container_network_input",
        "Container network input",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_NET_OUT: GaugeVec = register_gauge_vec!(
        "podman_container_network_output",
        "Container network output",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_BL_INP: GaugeVec = register_gauge_vec!(
        "podman_container_block_input",
        "Container block input",
        &["pod", "container"],
    )
    .unwrap();
    static ref CONTAINER_BL_OUT: GaugeVec = register_gauge_vec!(
        "podman_container_block_output",
        "Container block output",
        &["pod", "container"],
    )
    .unwrap();
}

#[derive(Debug)]
struct ContInfo {
    pod: Option<String>,
    name: String,
    state: isize,
}

struct Collector {
    podman: Podman,
}

impl Collector {
    fn new<U: AsRef<str>>(uri: U) -> Result<Self> {
        let podman = Podman::new(uri).map_err(|e| anyhow!("Create Podman interface: {}", e))?;
        Ok(Self { podman: podman })
    }

    async fn containers(&self) -> Result<HashMap<String, ContInfo>> {
        let containers_resp = self
            .podman
            .containers()
            .list(&ContainerListOpts::builder().all(true).build())
            .await
            .map_err(|e| anyhow!("Containers request: {}", e))?;
        let mut result = HashMap::new();
        for container in containers_resp {
            let id = match container.id {
                Some(id) => id,
                None => continue,
            };
            let pod = container.pod_name.filter(|v| v != "");
            let name = container
                .names
                .map(|ns| ns.first().map(String::from))
                .flatten();
            let name = match name {
                Some(n) => n,
                None => continue,
            };
            let state = match container.state.as_ref().map(String::as_ref) {
                Some("existed") => 0,
                Some("stopped") => 0,
                Some("running") => 1,
                Some("created") => 2,
                Some(_) | None => -1,
            };
            let info = ContInfo {
                pod: pod,
                name: name,
                state: state,
            };
            result.insert(id, info);
        }
        Ok(result)
    }

    async fn update_stat(&self) -> Result<()> {
        let containers = self.containers().await?;
        let resp = self
            .podman
            .containers()
            .stats(&Default::default())
            .await
            .map_err(|e| anyhow!("Stats request: {}", e))?;

        match resp.error {
            Value::Null => (),
            err @ _ => eprintln!("ApiError: {}", err),
        };
        let stats = match resp.stats {
            Some(stats) => stats,
            None => return Ok(()),
        };

        CONTAINER_TOTAL.set(containers.len() as f64);

        let mut pods: HashMap<String, usize> = HashMap::new();
        for (_, cont) in containers.iter() {
            if let Some(pod) = cont.pod.clone() {
                let container_cnt = pods.entry(pod).or_insert(0);
                *container_cnt += 1;
            }
        }
        for (pod, cnt) in pods.into_iter() {
            CONTAINER_COUNT.with_label_values(&[&pod]).set(cnt as f64);
        }

        for stat in stats.into_iter() {
            let cont_id = match stat.container_id.as_ref() {
                Some(id) => id,
                None => continue,
            };
            let cont = match containers.get(cont_id) {
                Some(s) => s,
                None => continue,
            };
            let pod = match cont.pod.as_ref() {
                Some(p) => p,
                None => "",
            };
            let name = &cont.name;

            CONTAINER_STATE
                .with_label_values(&[pod, name])
                .set(cont.state as f64);
            CONTAINER_UPTIME
                .with_label_values(&[pod, name])
                .set(stat.up_time.unwrap_or(0) as f64);
            CONTAINER_SYSTEM_NANO
                .with_label_values(&[pod, name])
                .set(stat.system_nano.unwrap_or(0) as f64);

            CONTAINER_PIDS
                .with_label_values(&[pod, name])
                .set(stat.pi_ds.unwrap_or(0) as f64);
            CONTAINER_AVG_CPU
                .with_label_values(&[pod, name])
                .set(stat.avg_cpu.unwrap_or(0.0) as f64);
            CONTAINER_CPU
                .with_label_values(&[pod, name])
                .set(stat.CPU.unwrap_or(0.0) as f64);
            CONTAINER_CPU_NANO
                .with_label_values(&[pod, name])
                .set(stat.cpu_nano.unwrap_or(0) as f64);
            CONTAINER_CPU_SYSTEM_NANO
                .with_label_values(&[pod, name])
                .set(stat.cpu_system_nano.unwrap_or(0) as f64);

            CONTAINER_MEM_USAGE
                .with_label_values(&[pod, name])
                .set(stat.mem_usage.unwrap_or(0) as f64);
            CONTAINER_MEM_LIMIT
                .with_label_values(&[pod, name])
                .set(stat.mem_limit.unwrap_or(0) as f64);
            CONTAINER_MEM_PERC
                .with_label_values(&[pod, name])
                .set(stat.mem_perc.unwrap_or(0.0) as f64);

            CONTAINER_NET_INP
                .with_label_values(&[pod, name])
                .set(stat.net_input.unwrap_or(0) as f64);
            CONTAINER_NET_OUT
                .with_label_values(&[pod, name])
                .set(stat.net_output.unwrap_or(0) as f64);
            CONTAINER_BL_INP
                .with_label_values(&[pod, name])
                .set(stat.block_input.unwrap_or(0) as f64);
            CONTAINER_BL_OUT
                .with_label_values(&[pod, name])
                .set(stat.block_output.unwrap_or(0) as f64);
        }
        Ok(())
    }
}

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    COLLECTOR.update_stat().await.unwrap();

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    Ok(response)
}

#[tokio::main]
async fn main() {
    let addr = IpAddr::from_str(&ARGS.host).unwrap();
    let host = (addr, ARGS.port).into();
    println!("Listening on http://{}", host);
    println!("Podman API {}", &ARGS.podman);

    let serve_future = Server::bind(&host).serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(serve_req))
    }));

    if let Err(err) = serve_future.await {
        eprintln!("server error: {}", err);
    }
}
