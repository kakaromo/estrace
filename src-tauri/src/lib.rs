// src-tauri/src/lib.rs

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
mod trace;
use std::collections::HashMap;
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ensure_db_exists().unwrap();
    {
        // 프로그램 시작 시 UFS_CACHE 재할당
        let mut ufs_cache = trace::UFS_CACHE.lock().expect("Failed to lock UFS_CACHE");
        *ufs_cache = HashMap::new();
        println!("✅ UFS_CACHE cleared at startup");
    }
    {
        // 프로그램 시작 시 BLOCK_CACHE 재할당
        let mut block_cache = trace::BLOCK_CACHE
            .lock()
            .expect("Failed to lock BLOCK_CACHE");
        *block_cache = HashMap::new();
        println!("✅ BLOCK_CACHE cleared at startup");
    }

    // Initialize default patterns - 수정된 호출 방식
    trace::initialize_patterns();

    tauri::Builder::default()
        .setup(|app| {
            // Ctrl/Cmd + Shift + I로 개발자 도구 토글
            #[cfg(desktop)]
            {
                use tauri::Manager;
                let window = app.get_webview_window("main").unwrap();
                
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_shortcut("CmdOrCtrl+Shift+I")?
                        .with_handler(move |_app, _shortcut, _event| {
                            if window.is_devtools_open() {
                                let _ = window.close_devtools();
                            } else {
                                let _ = window.open_devtools();
                            }
                        })
                        .build(),
                )?;
            }
            
            Ok(())
        })
        .enable_macos_default_menu(false)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            greet,
            trace::starttrace,
            trace::readtrace,
            trace::readtrace_highperf,  // 고성능 파서 명령 추가
            trace::readtrace_to_files,  // 파일 기반 데이터 전송
            trace::trace_lengths,
            trace::ufs_latencystats,
            trace::block_latencystats,
            trace::ufs_sizestats,
            trace::block_sizestats,
            trace::ufs_allstats,
            trace::block_allstats,
            trace::ufs_continuity_stats,
            trace::block_continuity_stats,
            trace::ufscustom_latencystats,
            trace::ufscustom_sizestats,
            trace::ufscustom_allstats,
            trace::ufscustom_continuity_stats,
            trace::export_to_csv,
            trace::filter_trace,
            // Pattern management commands
            trace::add_pattern,
            trace::set_active_pattern,
            trace::get_patterns,
            trace::get_active_patterns,
            trace::delete_pattern,
            // Reparse command
            trace::reparse_trace,
            trace::delete_parquet_files,
            // Pattern testing
            trace::test_regex_pattern,
            trace::delete_folder,
            trace::cancel_trace_process,
            trace::reset_cancel_signal,
            trace::check_cancel_status,
            // Cache management
            trace::clear_all_cache,
            trace::cleanup_temp_arrow_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
