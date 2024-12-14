#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
use notedeck_chrome::{setup::generate_native_options, Notedeck};

use notedeck::{DataPath, DataPathType};
use notedeck_columns::Damus;
use std::path::PathBuf;
use std::str::FromStr;
use tracing_subscriber::EnvFilter;

// Entry point for wasm
//#[cfg(target_arch = "wasm32")]
//use wasm_bindgen::prelude::*;

fn setup_logging(path: &DataPath) {
    #[allow(unused_variables)] // need guard to live for lifetime of program
    let (maybe_non_blocking, maybe_guard) = {
        let log_path = path.path(DataPathType::Log);
        // Setup logging to file

        use tracing_appender::{
            non_blocking,
            rolling::{RollingFileAppender, Rotation},
        };

        let file_appender = RollingFileAppender::new(
            Rotation::DAILY,
            log_path,
            format!("notedeck-{}.log", env!("CARGO_PKG_VERSION")),
        );

        let (non_blocking, _guard) = non_blocking(file_appender);

        (Some(non_blocking), Some(_guard))
    };

    // Log to stdout (if you run with `RUST_LOG=debug`).
    if let Some(non_blocking_writer) = maybe_non_blocking {
        use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

        let console_layer = fmt::layer().with_target(true).with_writer(std::io::stdout);

        // Create the file layer (writes to the file)
        let file_layer = fmt::layer()
            .with_ansi(false)
            .with_writer(non_blocking_writer);

        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("notedeck=info"));

        // Set up the subscriber to combine both layers
        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }
}

// Desktop
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    let base_path = DataPath::default_base().unwrap_or(PathBuf::from_str(".").unwrap());
    let path = DataPath::new(&base_path);

    setup_logging(&path);

    let _res = eframe::run_native(
        "Damus Notedeck",
        generate_native_options(path),
        Box::new(|cc| {
            let args: Vec<String> = std::env::args().collect();
            let mut notedeck = Notedeck::new(&cc.egui_ctx, base_path, &args);

            let damus = Damus::new(&mut notedeck.app_context(), &args);
            notedeck.add_app(damus);

            Ok(Box::new(notedeck))
        }),
    );
}

/*
 * TODO: nostrdb not supported on web
 *
#[cfg(target_arch = "wasm32")]
pub fn main() {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    wasm_bindgen_futures::spawn_local(async {
        let web_options = eframe::WebOptions::default();
        eframe::start_web(
            "the_canvas_id", // hardcode it
            web_options,
            Box::new(|cc| Box::new(Damus::new(cc, "."))),
        )
        .await
        .expect("failed to start eframe");
    });
}
*/

#[cfg(test)]
mod tests {
    use super::{Damus, Notedeck};
    use std::path::{Path, PathBuf};

    fn create_tmp_dir() -> PathBuf {
        tempfile::TempDir::new()
            .expect("tmp path")
            .path()
            .to_path_buf()
    }

    fn rmrf(path: impl AsRef<Path>) {
        let _ = std::fs::remove_dir_all(path);
    }

    /// Ensure dbpath actually sets the dbpath correctly.
    #[tokio::test]
    async fn test_dbpath() {
        let datapath = create_tmp_dir();
        let dbpath = create_tmp_dir();
        let args: Vec<String> = vec![
            "--testrunner",
            "--datapath",
            &datapath.to_str().unwrap(),
            "--dbpath",
            &dbpath.to_str().unwrap(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let ctx = egui::Context::default();
        let _app = Notedeck::new(&ctx, &datapath, &args);

        assert!(Path::new(&dbpath.join("data.mdb")).exists());
        assert!(Path::new(&dbpath.join("lock.mdb")).exists());
        assert!(!Path::new(&datapath.join("db")).exists());

        rmrf(datapath);
        rmrf(dbpath);
    }

    #[tokio::test]
    async fn test_column_args() {
        let tmpdir = create_tmp_dir();
        let npub = "npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s";
        let args: Vec<String> = vec![
            "--testrunner",
            "--no-keystore",
            "--pub",
            npub,
            "-c",
            "notifications",
            "-c",
            "contacts",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let ctx = egui::Context::default();
        let mut notedeck = Notedeck::new(&ctx, &tmpdir, &args);
        let mut app_ctx = notedeck.app_context();
        let app = Damus::new(&mut app_ctx, &args);

        assert_eq!(app.columns(app_ctx.accounts).columns().len(), 2);

        let tl1 = app
            .columns(app_ctx.accounts)
            .column(0)
            .router()
            .top()
            .timeline_id();

        let tl2 = app
            .columns(app_ctx.accounts)
            .column(1)
            .router()
            .top()
            .timeline_id();

        assert_eq!(tl1.is_some(), true);
        assert_eq!(tl2.is_some(), true);

        let timelines = app.columns(app_ctx.accounts).timelines();
        assert!(timelines[0].kind.is_notifications());
        assert!(timelines[1].kind.is_contacts());

        rmrf(tmpdir);
    }
}