// 桌面鱼塘 · Tauri 主进程
// 每台显示器一个透明无边框置顶窗口（鼠标穿透），托盘菜单 + 全局快捷键交互。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

type TankStats = (u32, u32, u32, u32, u32, u32); // alive, born, eaten, old, foods, sharks

struct AppState {
    interactive: AtomicBool,
    stats: Mutex<HashMap<String, TankStats>>,
}

fn sc(code: Code) -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), code)
}

fn is_interactive(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|s| s.interactive.load(Ordering::SeqCst))
        .unwrap_or(false)
}

// 当前显示器布局快照：(x, y, w, h)（物理像素）
fn monitors_snapshot(app: &AppHandle) -> Vec<(i32, i32, u32, u32)> {
    app.available_monitors()
        .map(|ms| {
            ms.iter()
                .map(|m| (m.position().x, m.position().y, m.size().width, m.size().height))
                .collect()
        })
        .unwrap_or_default()
}

// 为每台显示器创建一个鱼缸窗口
fn spawn_windows(app: &AppHandle) {
    // 先关闭旧的鱼缸窗口
    let old: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("tank"))
        .cloned()
        .collect();
    for label in old {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.close();
        }
    }

    let Ok(monitors) = app.available_monitors() else {
        println!("[aqua] ERROR: no monitors");
        return;
    };
    if monitors.is_empty() {
        println!("[aqua] WARN: empty monitor list, keep existing windows");
        return;
    }
    let interactive = is_interactive(app);

    for (i, m) in monitors.iter().enumerate() {
        let label = format!("tank-{}", i);
        let sf = m.scale_factor();
        let pos = m.position();
        let size = m.size();
        // 逻辑坐标（注入渲染层做全局->局部换算）
        let lx = pos.x as f64 / sf;
        let ly = pos.y as f64 / sf;
        let lw = size.width as f64 / sf;
        let lh = size.height as f64 / sf;
        let url = WebviewUrl::App(format!("index.html?ox={}&oy={}&label={}", lx, ly, label).into());
        let res = WebviewWindowBuilder::new(app, &label, url)
            .title("")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .resizable(false)
            .visible_on_all_workspaces(true)
            .focused(false)
            .inner_size(lw, lh)
            .position(lx, ly)
            .build();
        match res {
            Ok(win) => {
                let _ = win.set_ignore_cursor_events(!interactive);
                println!(
                    "[aqua] {} monitor=({},{} {}x{}) logical=({},{} {}x{}) sf={}",
                    label, pos.x, pos.y, size.width, size.height, lx, ly, lw, lh, sf
                );
            }
            Err(e) => println!("[aqua] ERROR create {}: {}", label, e),
        }
    }
}

fn set_interactive(app: &AppHandle, v: bool) {
    if let Some(state) = app.try_state::<AppState>() {
        state.interactive.store(v, Ordering::SeqCst);
    }
    for (label, win) in app.webview_windows() {
        if label.starts_with("tank") {
            let _ = win.set_ignore_cursor_events(!v);
            if v {
                let _ = win.set_focus();
            }
            let _ = win.emit("mode-changed", serde_json::json!({ "interactive": v }));
        }
    }
}

// 鼠标全局逻辑坐标（NSEvent.mouseLocation，无需辅助功能权限）
fn cursor_logical(app: &AppHandle) -> Option<(f64, f64)> {
    let p = app.cursor_position().ok()?;
    let sf = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    Some((p.x / sf, p.y / sf))
}

// 广播给所有窗口，渲染层按各自原点换算并忽略视口外事件
fn emit_at_cursor(app: &AppHandle, kind: &str) {
    let Some((gx, gy)) = cursor_logical(app) else { return };
    for (label, win) in app.webview_windows() {
        if label.starts_with("tank") {
            let _ = win.emit(
                "action",
                serde_json::json!({ "type": kind, "gx": gx, "gy": gy }),
            );
        }
    }
}

fn emit_simple(app: &AppHandle, kind: &str) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("tank") {
            let _ = win.emit("action", serde_json::json!({ "type": kind }));
        }
    }
}

#[tauri::command]
fn exit_interactive(app: AppHandle) {
    set_interactive(&app, false);
}

#[tauri::command]
fn report_size(w: f64, h: f64, search: String) {
    println!("[aqua] renderer {}x{} {}", w, h, search);
}

// 各缸汇报统计 -> 汇总到 macOS 菜单栏标题（类似网速显示）
#[tauri::command]
fn report_stats(
    app: AppHandle,
    label: String,
    alive: u32,
    born: u32,
    eaten: u32,
    old: u32,
    foods: u32,
    sharks: u32,
) {
    let Some(state) = app.try_state::<AppState>() else { return };
    {
        let mut map = state.stats.lock().unwrap();
        map.insert(label, (alive, born, eaten, old, foods, sharks));
        let (mut a, mut b, mut d, mut s) = (0, 0, 0, 0);
        for v in map.values() {
            a += v.0;
            b += v.1;
            d += v.2 + v.3;
            s += v.5;
        }
        let title = if s > 0 {
            format!("鱼{} · 生{} · 亡{} · 鲨{}", a, b, d, s)
        } else {
            format!("鱼{} · 生{} · 亡{}", a, b, d)
        };
        if let Some(tray) = app.tray_by_id("aqua-tray") {
            let _ = tray.set_title(Some(&title));
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([sc(Code::KeyF), sc(Code::KeyD), sc(Code::KeyX), sc(Code::KeyP)])
                .expect("register shortcuts")
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if *shortcut == sc(Code::KeyF) {
                        set_interactive(app, !is_interactive(app));
                    } else if *shortcut == sc(Code::KeyD) {
                        emit_at_cursor(app, "food");
                    } else if *shortcut == sc(Code::KeyX) {
                        emit_at_cursor(app, "shark");
                    } else if *shortcut == sc(Code::KeyP) {
                        emit_simple(app, "pause");
                    }
                })
                .build(),
        )
        .manage(AppState {
            interactive: AtomicBool::new(false),
            stats: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![exit_interactive, report_size, report_stats])
        .setup(|app| {
            // macOS：Accessory 模式，隐藏 Dock 图标（纯托盘应用）
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            spawn_windows(&handle);

            // 托盘
            let toggle =
                MenuItemBuilder::with_id("interactive", "进入/退出交互模式（Ctrl+Alt+F）").build(app)?;
            let food = MenuItemBuilder::with_id("food", "撒鱼食（Ctrl+Alt+D）").build(app)?;
            let shark = MenuItemBuilder::with_id("shark", "召唤鲨鱼（Ctrl+Alt+X）").build(app)?;
            let pause = MenuItemBuilder::with_id("pause", "暂停 / 继续（Ctrl+Alt+P）").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&toggle, &food, &shark, &pause, &quit])
                .build()?;
            TrayIconBuilder::with_id("aqua-tray")
                .icon(Image::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.png")).expect("tray icon"))
                .title("鱼…")
                .menu(&menu)
                .tooltip("桌面鱼塘")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "interactive" => set_interactive(app, !is_interactive(app)),
                    "food" => emit_at_cursor(app, "food"),
                    "shark" => emit_at_cursor(app, "shark"),
                    "pause" => emit_simple(app, "pause"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // 监听显示器布局变化（轮询），变化时重建鱼缸窗口
            let watcher = handle.clone();
            std::thread::spawn(move || {
                let mut last = monitors_snapshot(&watcher);
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let cur = monitors_snapshot(&watcher);
                    // 空快照是瞬态（如显示器休眠），忽略，避免误关所有鱼缸导致应用退出
                    if cur.is_empty() {
                        continue;
                    }
                    if cur != last {
                        println!("[aqua] monitors changed: {:?} -> {:?}", last, cur);
                        let h2 = watcher.clone();
                        let _ = watcher.run_on_main_thread(move || spawn_windows(&h2));
                        last = cur;
                    }
                }
            });

            // 轮询全局鼠标位置（30Hz）广播给各缸：穿透模式下鱼也能躲鼠标
            let cursor_watcher = handle.clone();
            std::thread::spawn(move || {
                let mut last = (f64::MIN, f64::MIN);
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(33));
                    if let Some((gx, gy)) = cursor_logical(&cursor_watcher) {
                        let dx = gx - last.0;
                        let dy = gy - last.1;
                        if dx * dx + dy * dy > 4.0 {
                            last = (gx, gy);
                            for (label, win) in cursor_watcher.webview_windows() {
                                if label.starts_with("tank") {
                                    let _ = win.emit(
                                        "cursor",
                                        serde_json::json!({ "gx": gx, "gy": gy }),
                                    );
                                }
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running desktop-aquarium");
}
