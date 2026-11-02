//! `jitter run <config>` collects until SIGTERM or SIGINT. `jitter check <config>` validates the
//! config, opens every source, and exits.

use jitter::collectors::{self, CollectError};
use jitter::config::Config;
use jitter::driver::{self, Driver};
use jitter::log::{self, Level};
use jitter::self_report::SelfReport;
use jitter::sink::{SinkHandle, SinkStats};
use jitter::{EXIT_CONFIG, EXIT_SOURCE, EXIT_USAGE};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const USAGE: &str =
    "usage: jitter run <config.toml> | jitter check <config.toml> | jitter --version";

fn main() -> ExitCode {
    if !cfg!(target_os = "linux") {
        log::event(
            Level::Error,
            "unsupported_os",
            &[(
                "detail",
                &"Jitter reads /proc and /sys and runs on Linux only",
            )],
        );
        return ExitCode::from(EXIT_SOURCE);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let (mode, path) = match args.as_slice() {
        [mode, path] if mode == "run" || mode == "check" => (mode.as_str(), PathBuf::from(path)),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let config = match Config::load(&path, Config::kernel_facts()) {
        Ok(c) => c,
        Err(e) => {
            log::event(Level::Error, "config_invalid", &[("error", &e)]);
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let collectors = match collectors::build(&config) {
        Ok(c) => c,
        Err(e) => return source_failed(&e),
    };
    let wakeups = driver::wakeups_per_window(&collectors, config.window.seconds);

    if mode == "check" {
        let probe =
            config.collectors.self_report.enabled.then(|| {
                SelfReport::open(&config.paths.proc, wakeups, Arc::new(SinkStats::default()))
            });
        if let Some(Err(e)) = probe {
            return source_failed(&e);
        }
        if let Err(e) = jitter::sink::check(&config.sink) {
            log::event(Level::Error, "sink_invalid", &[("error", &e)]);
            return ExitCode::from(EXIT_CONFIG);
        }
        log::event(
            Level::Info,
            "check_passed",
            &[("collectors", &collectors.len())],
        );
        return ExitCode::SUCCESS;
    }

    #[cfg(target_os = "linux")]
    if let Err(e) = jitter::sys::set_timer_slack(config.driver.timer_slack_ns) {
        log::event(Level::Error, "timer_slack_failed", &[("error", &e)]);
        return ExitCode::from(EXIT_CONFIG);
    }

    let mut sink = match SinkHandle::start(&config.sink) {
        Ok(s) => s,
        Err(e) => {
            log::event(Level::Error, "sink_invalid", &[("error", &e)]);
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let self_report = if config.collectors.self_report.enabled {
        match SelfReport::open(&config.paths.proc, wakeups, sink.stats()) {
            Ok(r) => Some(r),
            Err(e) => return source_failed(&e),
        }
    } else {
        None
    };

    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(e) = signal_hook::flag::register(signal, Arc::clone(&stop)) {
            log::event(Level::Error, "signal_setup_failed", &[("error", &e)]);
            return ExitCode::from(EXIT_SOURCE);
        }
    }

    let names: Vec<&str> = collectors.iter().map(|c| c.name()).collect();
    log::event(
        Level::Info,
        "started",
        &[
            ("version", &env!("CARGO_PKG_VERSION")),
            ("collectors", &names.join(",")),
            ("window_seconds", &config.window.seconds),
        ],
    );
    let driver = Driver::new(
        collectors,
        self_report,
        config.window.seconds,
        config.identity.host.clone(),
        stop,
    );
    let result = driver.run(&mut sink);
    match result {
        Ok(()) => {
            sink.finish();
            log::event(Level::Info, "stopped", &[]);
            ExitCode::SUCCESS
        }
        Err(e) => source_failed(&e),
    }
}

fn source_failed(e: &CollectError) -> ExitCode {
    log::event(Level::Error, "source_failed", &[("error", e)]);
    ExitCode::from(EXIT_SOURCE)
}
