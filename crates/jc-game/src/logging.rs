
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use bevy::app::App;
use bevy::log::BoxedLayer;
use bevy::log::tracing_subscriber;

/// The log of this start.
pub const LOG: &str = "jc-game.log";
/// The log of the start before.
pub const PREVIOUS: &str = "jc-game.prev.log";

/// The file the log layer takes when the log comes up.
static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Opens the log of a play session: the last one becomes the previous one, and the
/// panic hook is set. Gives where the log is, or why there is none.
pub fn start() -> Result<PathBuf, String> {
    let folder = crate::paths::logs();
    std::panic::set_hook(Box::new(crash_note));
    std::fs::create_dir_all(&folder).map_err(|err| format!("{}: {err}", folder.display()))?;
    let path = folder.join(LOG);
    if path.is_file() {
        // The rename puts the older log in place of the previous one.
        let _ = std::fs::rename(&path, folder.join(PREVIOUS));
    }
    let file = File::create(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    if let Ok(mut kept) = FILE.lock() {
        *kept = Some(file);
    }
    Ok(path)
}

/// The layer of Bevy's log that writes to the file of [`start`]; none without one.
pub fn file_layer(_app: &mut App) -> Option<BoxedLayer> {
    let file = FILE.lock().ok()?.take()?;
    let layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Mutex::new(file));
    Some(Box::new(layer))
}

/// The panic hook of a play session: the crash note, then what the default hook does.
fn crash_note(info: &std::panic::PanicHookInfo<'_>) {
    let backtrace = std::backtrace::Backtrace::force_capture();
    let thread = std::thread::current();
    let text = format!(
        "Jade Cocoon stopped with an error.\n\nthread: {}\n{info}\n\nbacktrace:\n{backtrace}\n",
        thread.name().unwrap_or("unnamed")
    );
    eprintln!("{text}");
    let folder = crate::paths::logs();
    let path = folder.join(format!("crash-{}.txt", stamp(std::time::SystemTime::now())));
    let written = std::fs::create_dir_all(&folder)
        .and_then(|()| File::create(&path))
        .and_then(|mut file| file.write_all(text.as_bytes()));
    if cfg!(not(debug_assertions)) {
        let said = match written {
            Ok(()) => format!(
                "Jade Cocoon stopped with an error.\n\nWhat happened is written in\n{}\n\nand \
                 the log of this start in\n{}",
                path.display(),
                folder.join(LOG).display()
            ),
            Err(err) => format!("Jade Cocoon stopped with an error:\n\n{info}\n\n({err})"),
        };
        crate::dialog::tell_error(&said);
    }
}

/// `2026-10-15_14-03-22` of a moment, in UTC.
fn stamp(moment: std::time::SystemTime) -> String {
    let seconds = moment
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The year, month and day of a count of days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
pub(crate) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_is_named_by_its_date_and_time() {
        let moment = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_791_986_602);
        assert_eq!(stamp(moment), "2026-10-14_14-03-22");
        assert_eq!(stamp(std::time::UNIX_EPOCH), "1970-01-01_00-00-00");
        assert_eq!(civil(11_016), (2000, 2, 29));
    }
}
