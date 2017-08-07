#![feature(box_syntax)]
#![feature(asm)]
extern crate e2d2;
//extern crate fnv;
//extern crate time;
extern crate getopts;
extern crate etcdv3_rs;
//extern crate rand;
//use e2d2::allocators::CacheAligned;

use e2d2::config::*;
//use e2d2::utils::Ipv4Prefix;
use e2d2::control::etcd_control::*;
//use e2d2::interface::*;
//use e2d2::operators::*;
use e2d2::scheduler::*;
use etcdv3_rs::etcd_proto::*;
use std::env;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const CONVERSION_FACTOR: f64 = 1000000000.;

fn subscriber_thread(name: &str, handle: WatcherHandle, s: &mut StandaloneScheduler) {
    let pfx = format!("{}:", name);
    let value = Arc::new(RwLock::new(String::from("")));
    let val_copy = value.clone();
    let updated = Arc::new(AtomicBool::new(false));
    let updated_copy = updated.clone();
    let mut current = String::from("");
    handle.watch_pfx(&pfx, move |triggers| {
        for trigger in triggers {
            if trigger.deleted {
                *(val_copy.write().unwrap()) = String::from("");
            } else {
                *(val_copy.write().unwrap()) = String::from_utf8(trigger.value.unwrap()).unwrap();
            }
            updated_copy.store(true, Ordering::Release);
        }
    });
    println!("Started watching");
    s.add_task(move || if updated.load(Ordering::Acquire) {
        current = (*(value.read().unwrap())).clone();
        updated.store(false, Ordering::Release);
    }).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut opts = basic_opts();
    opts.optopt("", "etcd", "etcd url", "url");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string()),
    };
    let configuration = read_matches(&matches, &opts);
    let etcd_url = matches.opt_str("etcd").unwrap_or_else(|| {
        String::from("http://127.0.0.1:2379")
    });

    let mut config = initialize_system(&configuration).unwrap();
    config.start_schedulers();
    let (watcher, handle) = Watcher::new(&etcd_url);
    let handle_clone = handle.clone();
    config.add_pipeline_to_run(Arc::new(move |_, s: &mut StandaloneScheduler| {
        println!("Starting pipeline");
        subscriber_thread("ntf", handle_clone.clone(), s);
    }));
    config.execute();
    println!("Running watcher");
    watcher.run();
    //config.execute();

    //let mut pkts_so_far = (0, 0);
    //let mut last_printed = 0.;
    //const MAX_PRINT_INTERVAL: f64 = 30.;
    //const PRINT_DELAY: f64 = 15.;
    //let sleep_delay = (PRINT_DELAY / 2.) as u64;
    //let mut start = time::precise_time_ns() as f64 / CONVERSION_FACTOR;
    //let sleep_time = Duration::from_millis(sleep_delay);
    //println!("0 OVERALL RX 0.00 TX 0.00 CYCLE_PER_DELAY 0 0 0");
    //loop {
    //thread::sleep(sleep_time); // Sleep for a bit
    //let now = time::precise_time_ns() as f64 / CONVERSION_FACTOR;
    //if now - start > PRINT_DELAY {
    //let mut rx = 0;
    //let mut tx = 0;
    //for port in config.ports.values() {
    //for q in 0..port.rxqs() {
    //let (rp, tp) = port.stats(q);
    //rx += rp;
    //tx += tp;
    //}
    //}
    //let pkts = (rx, tx);
    //let rx_pkts = pkts.0 - pkts_so_far.0;
    //if rx_pkts > 0 || now - last_printed > MAX_PRINT_INTERVAL {
    //println!(
    //"{:.2} OVERALL RX {:.2} TX {:.2}",
    //now - start,
    //rx_pkts as f64 / (now - start),
    //(pkts.1 - pkts_so_far.1) as f64 / (now - start)
    //);
    //last_printed = now;
    //start = now;
    //pkts_so_far = pkts;
    //}
    //}
    //}
}
