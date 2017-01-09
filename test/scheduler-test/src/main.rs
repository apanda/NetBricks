#![feature(box_syntax)]
#![feature(asm)]
#![feature(integer_atomics)]
extern crate e2d2;
extern crate fnv;
extern crate time;
extern crate getopts;
extern crate rand;
use e2d2::common::*;
use e2d2::config::*;
use e2d2::scheduler::*;
use getopts::Options;
use self::nf::*;
use std::env;
use std::thread;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicU64};
use std::time::Duration;
mod nf;

const CONVERSION_FACTOR: f64 = 1000000000.;

fn delay_task(d: u64) {
    delay_loop(d);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.optflag("h", "help", "print this help menu");
    opts.optmulti("c", "core", "Core to use", "core");
    opts.optopt("m", "master", "Master core", "master");
    opts.optopt("f", "configuration", "Configuration file", "path");
    opts.optopt("n", "name", "Process name", "name");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string()),
    };
    if matches.opt_present("h") {
        print!("{}", opts.usage(&format!("Usage: {} [options]", program)));
        process::exit(0)
    }

    let configuration = if matches.opt_present("f") {
        let config_file = matches.opt_str("f").unwrap();
        match read_configuration(&config_file[..]) {
            Ok(cfg) => cfg,
            Err(ref e) => {
                print_error(e);
                process::exit(1);
            }
        }
    } else {
        let name = matches.opt_str("n").unwrap_or_else(|| String::from("recv"));
        NetbricksConfiguration::new_with_name(&name[..])
    };

    let configuration = if matches.opt_present("m") {
        NetbricksConfiguration {
            primary_core: matches.opt_str("m")
                .unwrap()
                .parse()
                .expect("Could not parse master core"),
            strict: true,
            ..configuration
        }
    } else {
        configuration
    };

    let configuration = if matches.opt_present("c") {

        let cores_str = matches.opt_strs("c");

        let mut cores: Vec<i32> = cores_str.iter()
            .map(|n: &String| n.parse().ok().expect(&format!("Core cannot be parsed {}", n)))
            .collect();


        cores.dedup();
        NetbricksConfiguration {
            cores: cores,
            ..configuration
        }
    } else {
        configuration
    };

    println!("Going to start with configuration {}", configuration);

    // let mut config = initialize_system(&configuration)
    match initialize_system(&configuration) {
        Ok(mut context) => {
            context.start_schedulers();
            let core = context.active_cores[0];
            let x = Arc::new(AtomicU64::new(0));
            let y = Arc::new(AtomicU64::new(0));
            let t_x = x.clone();
            let t_y = y.clone();
            context.add_task_to_core(core, box move || { delay_task(100); t_x.fetch_add(1, Ordering::SeqCst); }).unwrap();
            context.add_task_to_core(core, box move || { delay_task(900); t_y.fetch_add(1, Ordering::SeqCst); }).unwrap();
            context.execute();

            const PRINT_DELAY: f64 = 15.;
            let sleep_delay = (PRINT_DELAY / 2.) as u64;
            let mut start = time::precise_time_ns() as f64 / CONVERSION_FACTOR;
            let sleep_time = Duration::from_millis(sleep_delay);
            //println!("0 OVERALL RX 0.00 TX 0.00 CYCLE_PER_DELAY 0 0 0");
            loop {
                thread::sleep(sleep_time); // Sleep for a bit
                let now = time::precise_time_ns() as f64 / CONVERSION_FACTOR;
                if now - start > PRINT_DELAY {
                    println!("{} x {} y {}", now - start, x.load(Ordering::SeqCst), y.load(Ordering::SeqCst));
                    start = now;
                }
            }
        }
        Err(ref e) => {
            println!("Error: {}", e);
            if let Some(backtrace) = e.backtrace() {
                println!("Backtrace: {:?}", backtrace);
            }
            process::exit(1);
        }
    }
}
