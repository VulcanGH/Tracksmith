use crate::{batch::Batch, lifecycle::Desktop};
use std::{
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{Arc, atomic::Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};
struct Context {
    desktop: Arc<Desktop>,
    batch: Arc<Batch>,
    dir: PathBuf,
    icon: HICON,
    taskbar: u32,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn notify(window: HWND, icon: HICON, action: u32) -> bool {
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = window;
    data.uID = 1;
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_APP + 1;
    data.hIcon = icon;
    for (target, value) in data
        .szTip
        .iter_mut()
        .zip(wide("Tracksmith — Media metadata. Refined."))
    {
        *target = value;
    }
    unsafe { Shell_NotifyIconW(action, &data) != 0 }
}
pub(super) fn run(desktop: Arc<Desktop>, batch: Arc<Batch>, dir: PathBuf) -> anyhow::Result<()> {
    unsafe {
        let module = GetModuleHandleW(null());
        let class = wide("Tracksmith.Tray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        anyhow::ensure!(RegisterClassW(&wc) != 0, "Cannot register tray window");
        let mut and = [0xffu8; 128];
        let mut xor = [0u8; 128];
        // Three geometric media tracks with offset markers; transparent outside white strokes.
        for y in 0..32 {
            for x in 0..32 {
                let mark = [(7, 10), (15, 21), (23, 15)].iter().any(|(line, marker)| {
                    ((y == *line || y == line + 1) && (4..28).contains(&x))
                        || ((x == *marker || x == marker + 1) && y >= line - 3 && y <= line + 4)
                });
                if mark {
                    let index = y * 4 + x / 8;
                    let bit = 0x80 >> (x % 8);
                    and[index] &= !bit;
                    xor[index] |= bit;
                }
            }
        }
        let icon = CreateIcon(module, 32, 32, 1, 1, and.as_ptr(), xor.as_ptr());
        anyhow::ensure!(!icon.is_null(), "Cannot create tray icon");
        let context = Box::new(Context {
            desktop,
            batch,
            dir,
            icon,
            taskbar: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        });
        let raw = Box::into_raw(context);
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Tracksmith").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            raw.cast(),
        );
        if window.is_null() {
            drop(Box::from_raw(raw));
            DestroyIcon(icon);
            anyhow::bail!("Cannot create tray window");
        }
        if !notify(window, icon, NIM_ADD) {
            DestroyWindow(window);
            anyhow::bail!("Windows rejected tray icon");
        }
        SetTimer(window, 1, 300, None);
        let mut message = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let ptr = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Context;
        if ptr.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let context = &*ptr;
        if message == context.taskbar {
            notify(window, context.icon, NIM_ADD);
            return 0;
        }
        match message {
            WM_TIMER if context.desktop.closing.load(Ordering::SeqCst) => {
                DestroyWindow(window);
                0
            }
            m if m == WM_APP + 1 => {
                if lparam as u32 == WM_LBUTTONDBLCLK {
                    let _ = super::open_url(&context.desktop.url);
                }
                if lparam as u32 == WM_RBUTTONUP || lparam as u32 == WM_CONTEXTMENU {
                    let menu = CreatePopupMenu();
                    for (id, label) in [
                        (1, "Open Tracksmith"),
                        (2, "Open media file…"),
                        (3, "Open media folder…"),
                        (4, "Open logs"),
                        (5, "About"),
                        (7, "Check for updates"),
                        (6, "Exit after current file"),
                    ] {
                        AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
                    }
                    let mut point = std::mem::zeroed();
                    GetCursorPos(&mut point);
                    SetForegroundWindow(window);
                    let selected = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON,
                        point.x,
                        point.y,
                        0,
                        window,
                        null(),
                    );
                    DestroyMenu(menu);
                    let outcome = (|| -> anyhow::Result<()> {
                        match selected {
                            1 => super::open_url(&context.desktop.url)?,
                            2 | 3 => {
                                if let Some(path) =
                                    super::picker::pick_for_owner(window, selected == 3)?
                                {
                                    context.desktop.enqueue(&path)?;
                                    super::open_url(&context.desktop.url)?;
                                }
                            }
                            4 => {
                                let logs = context.dir.join("logs");
                                std::fs::create_dir_all(&logs)?;
                                super::open_url(&logs.display().to_string())?;
                            }
                            5 => super::open_url(&format!("{}&page=About", context.desktop.url))?,
                            7 => super::open_url(&format!(
                                "{}&page=About&check=1",
                                context.desktop.url
                            ))?,
                            6 => {
                                context.batch.cancel.store(true, Ordering::SeqCst);
                                context.desktop.stop();
                            }
                            _ => {}
                        }
                        Ok(())
                    })();
                    if let Err(error) = outcome {
                        MessageBoxW(
                            window,
                            wide(&format!("{error:#}")).as_ptr(),
                            wide("Tracksmith").as_ptr(),
                            MB_OK | MB_ICONERROR,
                        );
                    }
                    PostMessageW(window, WM_NULL, 0, 0);
                }
                0
            }
            WM_DESTROY => {
                notify(window, context.icon, NIM_DELETE);
                DestroyIcon(context.icon);
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}
