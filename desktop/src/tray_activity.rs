//! Native tray activity continues when the webview is hidden.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Sender},
    Arc,
};
use std::time::Duration;
use tauri::Manager;

const IDLE_TOOLTIP: &str = "Nauctions — collection continues while this app is in the tray";
pub const TRAY_ID: &str = "nauctions";

pub struct Activity {
    count: Arc<AtomicUsize>,
    wake: Sender<()>,
}

pub struct Work(Arc<AtomicUsize>);
impl Drop for Work {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Activity {
    fn begin(&self) -> Work {
        if self.count.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = self.wake.send(());
        }
        Work(self.count.clone())
    }
}

pub fn begin(app: &tauri::AppHandle) -> Work {
    app.state::<Activity>().begin()
}

fn frame(index: usize) -> tauri::image::Image<'static> {
    let mut rgba = vec![0; 32 * 32 * 4];
    for (bar, height) in [10., 18., 14.].iter().enumerate() {
        let phase = index as f64 / 24. * std::f64::consts::TAU;
        let scale = 0.65 + 0.35 * (phase + bar as f64 * 2.1).cos();
        let height = (height * scale).round() as usize;
        for y in (25 - height)..25 {
            for x in (7 + bar * 7)..(11 + bar * 7) {
                let offset = (y * 32 + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[43, 133, 239, 255]);
            }
        }
    }
    tauri::image::Image::new_owned(rgba, 32, 32)
}

pub fn start(app: &tauri::AppHandle) {
    let (wake, receiver) = mpsc::channel();
    let count = Arc::new(AtomicUsize::new(0));
    app.manage(Activity {
        count: count.clone(),
        wake,
    });
    let app = app.clone();
    std::thread::spawn(move || {
        let frames: Vec<_> = (0..24).map(frame).collect();
        // No timer runs while idle. Add-on work wakes the animator.
        while receiver.recv().is_ok() {
            let Some(tray) = app.tray_by_id(TRAY_ID) else {
                return;
            };
            let _ = tray.set_tooltip(Some("Nauctions — preparing add-on data…"));
            let mut index = 0;
            while count.load(Ordering::SeqCst) > 0 {
                let _ = tray.set_icon(Some(frames[index % frames.len()].clone()));
                index += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = tray.set_icon(app.default_window_icon().cloned());
            let _ = tray.set_tooltip(Some(IDLE_TOOLTIP));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_work_and_error_returns_release_activity() {
        let (wake, _receiver) = mpsc::channel();
        let activity = Activity {
            count: Arc::new(AtomicUsize::new(0)),
            wake,
        };
        let first = activity.begin();
        let failed = || -> Result<(), ()> {
            let _work = activity.begin();
            assert_eq!(activity.count.load(Ordering::SeqCst), 2);
            Err(())
        };
        assert!(failed().is_err());
        assert_eq!(activity.count.load(Ordering::SeqCst), 1);
        drop(first);
        assert_eq!(activity.count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn animation_keeps_a_clear_background_and_changes_each_bar() {
        let first = frame(0);
        assert_eq!(first.rgba().len(), 32 * 32 * 4);
        assert_eq!(&first.rgba()[..4], &[0, 0, 0, 0]);
        for bar in 0..3 {
            let heights: Vec<_> = (0..24)
                .map(|index| {
                    let image = frame(index);
                    (0..32)
                        .filter(|y| image.rgba()[(y * 32 + 7 + bar * 7) * 4 + 3] > 0)
                        .count()
                })
                .collect();
            assert!(heights.iter().min() < heights.iter().max());
        }
    }
}
