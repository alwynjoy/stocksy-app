#![recursion_limit = "256"]
pub mod commands;
pub mod db;
pub mod dcf;
pub mod debug_log;
pub mod engine;
pub mod keyring_store;
pub mod llm;
pub mod pipeline;
pub mod prompts;
pub mod queue;
pub mod pricing;
pub mod backup;
pub mod seeds;
pub mod stubs;
pub mod ticker;
pub mod tools;
pub mod yahoo;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};

pub const PKG_NAME: &str = "Stocksy Desktop";
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

const MENU_DASHBOARD: &str = "menu.dashboard";
const MENU_MY_REPORTS: &str = "menu.my_reports";
const MENU_SCREENER: &str = "menu.screener";
const MENU_WATCHLISTS: &str = "menu.watchlists";
const MENU_NEW_ANALYSIS: &str = "menu.new_analysis";
const MENU_SETTINGS: &str = "menu.settings";
const MENU_SETTINGS_LLM: &str = "menu.settings.llm";
const MENU_SETTINGS_TOOLS: &str = "menu.settings.tools";
const MENU_SETTINGS_COUNTRIES: &str = "menu.settings.countries";
const MENU_SETTINGS_PROMPTS: &str = "menu.settings.prompts";
const MENU_SETTINGS_APPEARANCE: &str = "menu.settings.appearance";
const MENU_SETTINGS_DATA: &str = "menu.settings.data";
const MENU_SETTINGS_ANALYSIS: &str = "menu.settings.analysis";
const MENU_HELP_DCF: &str = "menu.help.dcf";

pub struct DbState {
    pub conn: Arc<Mutex<Connection>>,
    pub path: PathBuf,
}

fn build_menu(app: &tauri::App) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::IsMenuItem;

    let new_analysis = MenuItem::with_id(
        app,
        MENU_NEW_ANALYSIS,
        "New Analysis",
        true,
        Some("CmdOrCtrl+N"),
    )?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit Stocksy Desktop"))?;

    let about = if cfg!(target_os = "macos") {
        Some(PredefinedMenuItem::about(
            app,
            Some("About Stocksy Desktop"),
            None,
        )?)
    } else {
        None
    };
    let hide = if cfg!(target_os = "macos") {
        Some(PredefinedMenuItem::hide(app, None)?)
    } else {
        None
    };
    let app_menu: Vec<&dyn IsMenuItem<tauri::Wry>> = match (about.as_ref(), hide.as_ref()) {
        (Some(a), Some(h)) => vec![a, &separator, &settings, &separator, h, &separator, &quit],
        _ => vec![&settings, &separator, &quit],
    };
    let app_submenu = Submenu::with_items(app, PKG_NAME, true, &app_menu)?;

    let dashboard = MenuItem::with_id(app, MENU_DASHBOARD, "Dashboard", true, None::<&str>)?;
    let my_reports = MenuItem::with_id(app, MENU_MY_REPORTS, "My Reports", true, None::<&str>)?;
    let screener = MenuItem::with_id(app, MENU_SCREENER, "Screener", true, None::<&str>)?;
    let watchlists = MenuItem::with_id(app, MENU_WATCHLISTS, "Watchlists", true, None::<&str>)?;

    let file_menu: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![
        &dashboard,
        &my_reports,
        &screener,
        &watchlists,
        &new_analysis,
    ];
    let file_submenu = Submenu::with_items(app, "File", true, &file_menu)?;

    let edit_undo = PredefinedMenuItem::undo(app, None)?;
    let edit_redo = PredefinedMenuItem::redo(app, None)?;
    let edit_sep = PredefinedMenuItem::separator(app)?;
    let edit_cut = PredefinedMenuItem::cut(app, None)?;
    let edit_copy = PredefinedMenuItem::copy(app, None)?;
    let edit_paste = PredefinedMenuItem::paste(app, None)?;
    let edit_select_all = PredefinedMenuItem::select_all(app, None)?;
    let edit_menu: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![
        &edit_undo,
        &edit_redo,
        &edit_sep,
        &edit_cut,
        &edit_copy,
        &edit_paste,
        &edit_sep,
        &edit_select_all,
    ];
    let edit_submenu = Submenu::with_items(app, "Edit", true, &edit_menu)?;

    let settings_llm = MenuItem::with_id(app, MENU_SETTINGS_LLM, "LLM Provider", true, None::<&str>)?;
    let settings_tools = MenuItem::with_id(app, MENU_SETTINGS_TOOLS, "AI Agent Tools", true, None::<&str>)?;
    let settings_countries =
        MenuItem::with_id(app, MENU_SETTINGS_COUNTRIES, "Country Parameters", true, None::<&str>)?;
    let settings_prompts =
        MenuItem::with_id(app, MENU_SETTINGS_PROMPTS, "System Prompts", true, None::<&str>)?;
    let settings_appearance =
        MenuItem::with_id(app, MENU_SETTINGS_APPEARANCE, "Miscellaneous", true, None::<&str>)?;
    let settings_data = MenuItem::with_id(app, MENU_SETTINGS_DATA, "Database & Logging", true, None::<&str>)?;
    let settings_analysis = MenuItem::with_id(app, MENU_SETTINGS_ANALYSIS, "Queue & Parallelism", true, None::<&str>)?;
    let settings_submenu = Submenu::with_items(
        app,
        "Settings",
        true,
        &[
            &settings,
            &separator,
            &settings_llm,
            &settings_tools,
            &settings_countries,
            &settings_prompts,
            &settings_data,
            &settings_analysis,
            &settings_appearance,
        ],
    )?;

    let help_dcf = MenuItem::with_id(app, MENU_HELP_DCF, "User Guide & Help", true, None::<&str>)?;
    let help_menu: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![&help_dcf];
    let help_submenu = Submenu::with_items(app, "Help", true, &help_menu)?;

    let root: Vec<&dyn IsMenuItem<tauri::Wry>> = if cfg!(target_os = "macos") {
        vec![
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &settings_submenu,
            &help_submenu,
        ]
    } else {
        // Windows/Linux: File carries Quit; Settings/Help as top-level items on the menubar.
        vec![&file_submenu, &settings_submenu, &help_submenu]
    };

    Menu::with_items(app, &root)
}

pub fn run() {
    let mut builder = tauri::Builder::default();

    builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let path = dir.join("stocksy.db");
            let conn = db::init(&path).expect("failed to initialize database");

            // Init queue with persisted max_parallel or default 1
            let max_parallel: usize = crate::db::get_setting(&conn, "analysis.max_parallel")
                .ok()
                .flatten()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 5);

            if let Some(geom) = db::load_window_geometry(&conn) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_size(PhysicalSize::new(geom.width, geom.height));
                    let _ = window.set_position(PhysicalPosition::new(geom.x, geom.y));
                }
            }

            app.manage(DbState {
                conn: Arc::new(Mutex::new(conn)),
                path: path.clone(),
            });
            app.manage(crate::queue::QueueHandle::new(max_parallel));

            let menu = build_menu(app)?;
            app.set_menu(menu)?;
            println!("[boot] stocksy db: {}", path.display());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::db_health,
            commands::get_setting,
            commands::set_setting,
            commands::list_settings,
            commands::get_secret,
            commands::set_secret,
            commands::delete_secret,
            commands::secret_status,
            commands::countries_list,
            commands::country_upsert,
            commands::prompts_list,
            commands::prompt_update,
            commands::prompt_restore_default,
            commands::prompt_restore_all,
            commands::dcf_defaults,
            commands::llm_fetch_models,
            commands::llm_test_connection,
            commands::data_info,
            commands::db_browser_tables,
            commands::db_browser_query,
            commands::ticker_search,
            commands::ticker_yahoo_search,
            commands::fetch_financial_data,
            commands::get_cached_financial_data,
            commands::check_financial_staleness,
            commands::list_reports,
            commands::get_report,
            commands::get_report_by_id,
            commands::get_report_versions,
            commands::get_reports_by_ids,
            commands::toggle_favorite,
            commands::delete_reports,
            commands::update_cmp,
            commands::update_price,
            commands::recompute_local,
            commands::run_analysis,
            commands::enqueue_analysis,
            commands::list_queue,
            commands::get_queue_state,
            commands::get_max_parallel,
            commands::set_max_parallel,
            commands::debug_log_dir,
            commands::debug_logs_list,
            commands::debug_log_read,
            commands::debug_logs_clear,
            commands::dashboard_stats,
            commands::screener_rows,
            commands::watchlists_list,
            commands::watchlist_create,
            commands::watchlist_update,
            commands::watchlist_delete,
            commands::watchlist_items,
            commands::watchlist_add_item,
            commands::watchlist_remove_item,
            commands::watchlist_update_item,
            commands::save_pdf_file,
            commands::save_backup_file,
            commands::backup_export,
            commands::backup_validate,
            commands::backup_restore,
            commands::is_historical_unchanged,
            commands::stub_dashboard_stats,
            commands::stub_screener_rows,
        ])
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_DASHBOARD => {
                let _ = app.emit("menu://nav", "/");
            }
            MENU_MY_REPORTS => {
                let _ = app.emit("menu://nav", "/reports");
            }
            MENU_SCREENER => {
                let _ = app.emit("menu://nav", "/screener");
            }
            MENU_WATCHLISTS => {
                let _ = app.emit("menu://nav", "/watchlists");
            }
            MENU_NEW_ANALYSIS => {
                let _ = app.emit("menu://nav", "/new-analysis");
            }
            MENU_SETTINGS => {
                let _ = app.emit("menu://nav", "/settings");
            }
            MENU_SETTINGS_LLM => {
                let _ = app.emit("menu://nav", "/settings?tab=llm");
            }
            MENU_SETTINGS_TOOLS => {
                let _ = app.emit("menu://nav", "/settings?tab=tools");
            }
            MENU_SETTINGS_COUNTRIES => {
                let _ = app.emit("menu://nav", "/settings?tab=countries");
            }
            MENU_SETTINGS_PROMPTS => {
                let _ = app.emit("menu://nav", "/settings?tab=prompts");
            }
            MENU_SETTINGS_APPEARANCE => {
                let _ = app.emit("menu://nav", "/settings?tab=appearance");
            }
            MENU_SETTINGS_DATA => {
                let _ = app.emit("menu://nav", "/settings?tab=data");
            }
            MENU_SETTINGS_ANALYSIS => {
                let _ = app.emit("menu://nav", "/settings?tab=analysis");
            }
            MENU_HELP_DCF => {
                let _ = app.emit("menu://nav", "/help");
            }
            _ => {}
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                if let (Ok(size), Ok(pos)) = (window.outer_size(), window.outer_position()) {
                    let app = window.app_handle();
                    if let Some(state) = app.try_state::<DbState>() {
                        let guard = state.conn.lock().unwrap();
                        let _ = db::save_window_geometry(
                            &guard,
                            db::WindowGeometry {
                                width: size.width as i32,
                                height: size.height as i32,
                                x: pos.x,
                                y: pos.y,
                            },
                        );
                    }
                }
            }
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running Stocksy Desktop");
}
