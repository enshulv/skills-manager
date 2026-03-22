use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};

mod commands;
mod core;

/// Shared flag: when true, CloseRequested should NOT be prevented.
pub static QUITTING: AtomicBool = AtomicBool::new(false);

fn restore_main_window(app: &tauri::AppHandle) {
    let app_for_main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = app_for_main.set_dock_visibility(true) {
                log::error!("Failed to show Dock icon on macOS: {err}");
            }
            if let Err(err) = app_for_main.set_activation_policy(tauri::ActivationPolicy::Regular) {
                log::error!("Failed to set activation policy to Regular on macOS: {err}");
            }
            if let Err(err) = app_for_main.show() {
                log::error!("Failed to show app on macOS: {err}");
            }
        }

        if let Some(w) = app_for_main.get_webview_window("main") {
            if let Err(err) = w.show() {
                log::error!("Failed to show main window: {err}");
            }
            if let Err(err) = w.unminimize() {
                log::error!("Failed to unminimize main window: {err}");
            }
            if let Err(err) = w.set_focus() {
                log::error!("Failed to focus main window: {err}");
            }
        } else {
            log::error!("Main window not found while restoring from tray");
        }
    }) {
        log::error!("Failed to schedule restore_main_window on main thread: {err}");
    }
}

fn request_quit(app: &tauri::AppHandle) {
    let app_for_main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        quit_app(&app_for_main);
    }) {
        log::error!("Failed to schedule quit on main thread: {err}");
        // Fallback: attempt quit anyway.
        quit_app(app);
    }
}

/// Quit the application cleanly: destroy the main window, then exit.
/// In dev mode, also kill sibling processes in the same process group
/// so that `tauri dev`'s beforeDevCommand (vite) gets cleaned up.
pub fn quit_app(app: &tauri::AppHandle) {
    QUITTING.store(true, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("main") {
        if let Err(err) = w.destroy() {
            log::error!("Failed to destroy main window while quitting: {err}");
        }
    }
    // In dev mode, kill sibling processes (vite dev server) by signaling the process group.
    // Uses libc directly to avoid platform-specific `kill` command syntax differences.
    #[cfg(unix)]
    unsafe {
        // getpgrp() returns our process group ID; kill(-pgid, SIGTERM) sends to all in the group.
        let pgid = libc::getpgrp();
        libc::kill(-pgid, libc::SIGTERM);
    }
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure central repo exists
    core::central_repo::ensure_central_repo().expect("Failed to create central repo");

    // Initialize database
    let db_path = core::central_repo::db_path();
    let store = Arc::new(
        core::skill_store::SkillStore::new(&db_path).expect("Failed to initialize database"),
    );
    initialize_startup_scenario(&store).expect("Failed to initialize startup scenario");

    let cancel_registry = Arc::new(core::install_cancel::InstallCancelRegistry::new());

    tauri::Builder::default()
        .manage(store)
        .manage(cancel_registry)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // System tray
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let mut builder = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Skills Manager")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        log::info!("Tray menu clicked: show");
                        restore_main_window(app)
                    }
                    "quit" => {
                        log::info!("Tray menu clicked: quit");
                        request_quit(app)
                    }
                    _ => {}
                });

            // On macOS, left-click on tray icon opens the menu by default;
            // on Windows/Linux, left-click restores the window directly.
            if !cfg!(target_os = "macos") {
                builder = builder
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            restore_main_window(tray.app_handle());
                        }
                    });
            }

            let _tray = builder.build(app)?;

            // Intercept window close — let frontend decide (close vs hide to tray)
            // When QUITTING is set, allow the close to proceed so the process fully exits.
            let win = app.get_webview_window("main").unwrap();
            let win_for_event = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if QUITTING.load(Ordering::SeqCst) {
                        return; // allow close
                    }
                    win_for_event.emit("window-close-requested", ()).ok();
                    api.prevent_close();
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Tools
            commands::tools::get_tool_status,
            // Skills
            commands::skills::get_managed_skills,
            commands::skills::get_skills_for_scenario,
            commands::skills::get_skill_document,
            commands::skills::delete_managed_skill,
            commands::skills::install_local,
            commands::skills::install_git,
            commands::skills::install_from_skillssh,
            commands::skills::check_skill_update,
            commands::skills::check_all_skill_updates,
            commands::skills::update_skill,
            commands::skills::reimport_local_skill,
            commands::skills::get_all_tags,
            commands::skills::set_skill_tags,
            commands::skills::cancel_install,
            commands::skills::batch_import_folder,
            // Sync
            commands::sync::sync_skill_to_tool,
            commands::sync::unsync_skill_from_tool,
            // Scan
            commands::scan::scan_local_skills,
            commands::scan::import_existing_skill,
            commands::scan::import_all_discovered,
            // Browse
            commands::browse::fetch_leaderboard,
            commands::browse::search_skillssh,
            // Settings
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::get_central_repo_path,
            commands::settings::open_central_repo_folder,
            commands::settings::check_app_update,
            commands::settings::app_exit,
            commands::settings::hide_to_tray,
            // Git Backup
            commands::git_backup::git_backup_status,
            commands::git_backup::git_backup_init,
            commands::git_backup::git_backup_set_remote,
            commands::git_backup::git_backup_commit,
            commands::git_backup::git_backup_push,
            commands::git_backup::git_backup_pull,
            commands::git_backup::git_backup_clone,
            commands::git_backup::git_backup_create_snapshot,
            commands::git_backup::git_backup_list_versions,
            commands::git_backup::git_backup_restore_version,
            // Projects
            commands::projects::get_projects,
            commands::projects::add_project,
            commands::projects::remove_project,
            commands::projects::scan_projects,
            commands::projects::get_project_skills,
            commands::projects::get_project_skill_document,
            commands::projects::import_project_skill_to_center,
            commands::projects::export_skill_to_project,
            commands::projects::update_project_skill_to_center,
            commands::projects::update_project_skill_from_center,
            commands::projects::toggle_project_skill,
            commands::projects::delete_project_skill,
            commands::projects::slugify_skill_names,
            // Scenarios
            commands::scenarios::get_scenarios,
            commands::scenarios::get_active_scenario,
            commands::scenarios::create_scenario,
            commands::scenarios::update_scenario,
            commands::scenarios::delete_scenario,
            commands::scenarios::switch_scenario,
            commands::scenarios::add_skill_to_scenario,
            commands::scenarios::remove_skill_from_scenario,
            commands::scenarios::reorder_scenarios,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn initialize_startup_scenario(store: &Arc<core::skill_store::SkillStore>) -> Result<(), String> {
    let scenarios = store.get_all_scenarios().map_err(|e| e.to_string())?;
    if scenarios.is_empty() {
        return Ok(());
    }

    let current_active = store.get_active_scenario_id().map_err(|e| e.to_string())?;
    let preferred_default = store.get_setting("default_scenario").ok().flatten();

    let desired_active = preferred_default
        .filter(|id| scenarios.iter().any(|scenario| scenario.id == *id))
        .or_else(|| {
            current_active
                .clone()
                .filter(|id| scenarios.iter().any(|scenario| scenario.id == *id))
        })
        .unwrap_or_else(|| scenarios[0].id.clone());

    if current_active.as_deref() != Some(desired_active.as_str()) {
        if let Some(old_active) = current_active.as_deref() {
            commands::scenarios::unsync_scenario_skills(store, old_active)
                .map_err(|e| e.to_string())?;
        }

        store
            .set_active_scenario(&desired_active)
            .map_err(|e| e.to_string())?;
    }

    commands::scenarios::sync_scenario_skills(store, &desired_active)
        .map_err(|e| e.to_string())?;
    Ok(())
}
