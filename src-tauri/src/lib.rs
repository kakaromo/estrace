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
    }
    {
        // 프로그램 시작 시 BLOCK_CACHE 재할당
        let mut block_cache = trace::BLOCK_CACHE
            .lock()
            .expect("Failed to lock BLOCK_CACHE");
        *block_cache = HashMap::new();
    }

    // Initialize default patterns - 수정된 호출 방식
    trace::initialize_patterns();

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            greet,
            trace::starttrace,
            trace::readtrace,
            trace::ufs_latencystats,
            trace::block_latencystats,
            trace::ufs_sizestats,
            trace::block_sizestats,
            trace::ufs_continuity_stats,
            trace::block_continuity_stats,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}