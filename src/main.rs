// nvtray - NVIDIA GPU activity tray icon for Linux (StatusNotifierItem).
// Copyright (C) 2026 Vendetta1871
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General
// Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.

use std::time::Duration;

use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip, Tray, TrayMethods};
use nvml_wrapper::Nvml;

const POLL_INTERVAL: Duration = Duration::from_millis(1000);

// NVIDIA green
const GREEN_BORDER: (u8, u8, u8) = (0x72, 0xB9, 0x03);
const GREEN_PIN: (u8, u8, u8) = (0xBE, 0xDC, 0x50);
// Idle gray
const GRAY_BORDER: (u8, u8, u8) = (0xC0, 0xC0, 0xC0);
const GRAY_PIN: (u8, u8, u8) = (0xE0, 0xE0, 0xE0);
const GRAY_CELL: (u8, u8, u8) = (0xAD, 0xAD, 0xAD);
const DARK_FILL: (u8, u8, u8) = (0x2B, 0x2B, 0x2B);
// Active die gradient corners: TL pink, TR yellow, BL blue, BR green
const GRAD_TL: (u8, u8, u8) = (0xE2, 0x5A, 0xAA);
const GRAD_TR: (u8, u8, u8) = (0xF0, 0xC8, 0x3C);
const GRAD_BL: (u8, u8, u8) = (0x6E, 0x6E, 0xE6);
const GRAD_BR: (u8, u8, u8) = (0x46, 0xD7, 0x82);

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

/// Bilinear blend of the four gradient corners; u, v in [0, 1].
fn grad(u: f32, v: f32) -> (u8, u8, u8) {
    let mix = |i: usize| {
        let get = |c: (u8, u8, u8)| [c.0, c.1, c.2][i];
        let top = lerp(get(GRAD_TL), get(GRAD_TR), u);
        let bot = lerp(get(GRAD_BL), get(GRAD_BR), u);
        lerp(top, bot, v)
    };
    (mix(0), mix(1), mix(2))
}

/// Draws the classic NVIDIA "GPU Activity" icon: a chip with a framed grid
/// of small cells (rainbow gradient when active, gray when idle).
/// Returns ARGB32 (network byte order) pixels.
fn make_icon(size: i32, active: bool) -> Icon {
    let s = size as usize;
    let border = if active { GREEN_BORDER } else { GRAY_BORDER };
    let pin = if active { GREEN_PIN } else { GRAY_PIN };

    let mut px = vec![0u8; s * s * 4];
    let mut set = |x: usize, y: usize, c: (u8, u8, u8)| {
        let i = (y * s + x) * 4;
        px[i] = 0xFF; // A
        px[i + 1] = c.0; // R
        px[i + 2] = c.1; // G
        px[i + 3] = c.2; // B
    };

    // Frame + dark interior
    for y in 0..s {
        for x in 0..s {
            let c = if x == 0 || y == 0 || x == s - 1 || y == s - 1 {
                border
            } else {
                DARK_FILL
            };
            set(x, y, c);
        }
    }

    // Contact pins on the frame edges (dashes on the outermost pixels)
    for i in 2..s - 2 {
        if i % 4 < 2 {
            set(i, 0, pin);
            set(i, s - 1, pin);
            set(0, i, pin);
            set(s - 1, i, pin);
        }
    }

    // Grid of cells
    const COLS: usize = 5;
    const ROWS: usize = 4;
    let cell_sz = (s / 7).max(2);
    let gap = (s / 22).max(1);
    let gw = COLS * cell_sz + (COLS - 1) * gap;
    let gh = ROWS * cell_sz + (ROWS - 1) * gap;
    let ox = (s - gw) / 2;
    let oy = (s - gh) / 2;
    for r in 0..ROWS {
        for c in 0..COLS {
            let cell = if active {
                grad(
                    c as f32 / (COLS - 1) as f32,
                    r as f32 / (ROWS - 1) as f32,
                )
            } else {
                GRAY_CELL
            };
            for dy in 0..cell_sz {
                for dx in 0..cell_sz {
                    set(ox + c * (cell_sz + gap) + dx, oy + r * (cell_sz + gap) + dy, cell);
                }
            }
        }
    }

    Icon { width: size, height: size, data: px }
}

struct GpuTray {
    util: u32,
    mem: u32,
    active: bool,
}

/// Launches nvidia-settings detached, if it is installed.
fn open_settings() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if let Ok(mut child) = Command::new("nvidia-settings")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0) // detach from our session
        .spawn()
    {
        // Reap the child when it exits so it doesn't linger as a zombie.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

impl Tray for GpuTray {
    fn id(&self) -> String {
        "nvtray".into()
    }

    fn title(&self) -> String {
        "NVIDIA GPU Activity".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![make_icon(22, self.active), make_icon(16, self.active)]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: format!("NVIDIA GPU Activity: {}%", self.util),
            description: format!("Memory controller: {}%", self.mem),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open_settings();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Open Settings".into(),
                activate: Box::new(|_| open_settings()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

const REDETECT_INTERVAL: Duration = Duration::from_secs(3);

/// Waits until an NVIDIA GPU shows up in the system.
async fn wait_for_gpu() -> Nvml {
    loop {
        if let Ok(nvml) = Nvml::init() {
            if nvml.device_by_index(0).is_ok() {
                return nvml;
            }
        }
        tokio::time::sleep(REDETECT_INTERVAL).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    loop {
        // No tray icon at all while there is no NVIDIA GPU in the system.
        let nvml = wait_for_gpu().await;

        let tray = GpuTray { util: 0, mem: 0, active: false };
        let handle = match tray.spawn().await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("failed to spawn tray icon: {e}");
                tokio::time::sleep(REDETECT_INTERVAL).await;
                continue;
            }
        };

        // Poll until the GPU disappears (eGPU unplugged, driver unloaded...).
        loop {
            match nvml
                .device_by_index(0)
                .and_then(|d| d.utilization_rates())
            {
                Ok(u) => {
                    handle
                        .update(|t: &mut GpuTray| {
                            t.util = u.gpu;
                            t.mem = u.memory;
                            t.active = u.gpu > 0;
                        })
                        .await;
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                Err(_) => break,
            }
        }

        // GPU is gone: remove the icon from the tray and re-detect.
        handle.shutdown().await;
    }
}
