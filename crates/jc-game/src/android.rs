//! The game on an Android phone (`crates/jc-android`, `android/`).
//!
//! The APK's first screen (`DiscActivity.java`) asks for the disc image with the
//! system's file picker and copies it into the app's own folder as [`DISC`]; the game
//! starts behind it, in a process of its own (`GameActivity`), at [`android_main`]. The
//! app's own folder (`Android/data/io.github.ufodriverr.jadecocoon/files`, which a PC
//! reaches by USB, to copy the saves both ways) is then the working folder, so the game folder of `paths.rs`, the
//! `jc.local.toml` of `jc-disc` and its `cache` lie there as they lie beside the exe of
//! a PC. An image the game cannot read goes back to the first screen with the reason
//! ([`PROBLEM`]).

use std::path::PathBuf;

use bevy::android::android_activity::AndroidApp;
use bevy::log::{error, info};
use jc_disc::LOCAL_CONFIG_NAME;
use jni::JavaVM;
use jni::objects::{JObject, JString, JValue};

/// The copy of the disc image in the app's folder (`DiscActivity.java`).
pub const DISC: &str = "disc.bin";
/// Why the game did not take the image: the first screen says it and asks again.
pub const PROBLEM: &str = "disc_problem.txt";

/// Where the phone starts the game: the app's folder becomes the working folder, then
/// the game of a PC runs. The process ends with the game, so that the next start is a
/// fresh one.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let _ = bevy::android::ANDROID_APP.set(app.clone());
    if let Err(err) = prepare(&app) {
        error!("the app's folder: {err}");
    }
    let exit = crate::main();
    info!("the game ends: {exit:?}");
    std::process::exit(0);
}

/// The app's own folder as the working folder, and the disc image there in
/// `jc.local.toml` while the file names none.
fn prepare(app: &AndroidApp) -> Result<(), String> {
    // The app's folder on the shared storage, which a PC reaches by USB, as
    // `DiscActivity.gameFolder` finds it; the private one without that storage.
    let folder = (app.external_data_path())
        .or_else(|| app.internal_data_path())
        .ok_or("Android gave no folder for the app")?;
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    std::env::set_current_dir(&folder).map_err(|err| err.to_string())?;
    info!("the game folder: {}", folder.display());
    let _ = std::fs::remove_file(folder.join(PROBLEM));
    let config = folder.join(LOCAL_CONFIG_NAME);
    let disc = folder.join(DISC);
    if !config.is_file() && disc.is_file() {
        jc_disc::LocalConfig::store_disc_image(&folder, &disc).map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// The game cannot read the disc image: the reason is kept for the first screen, which
/// asks for another one at the next start.
pub fn disc_refused(err: &str) {
    error!("the disc image: {err}");
    let folder = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::write(folder.join(PROBLEM), err);
    let _ = std::fs::remove_file(folder.join(LOCAL_CONFIG_NAME));
    // The activity that asks comes back (`DiscActivity.java`).
    if let Err(err) = call_activity("askForDisc", "()V", &[]) {
        error!("cannot go back to the disc screen: {err}");
    }
}

/// Puts text on Android's clipboard (`MainActivity.setClipboard`).
pub fn set_clipboard(text: &str) -> Result<(), String> {
    with_activity(|env, activity| {
        let text = env.new_string(text).map_err(|err| err.to_string())?;
        env.call_method(
            activity,
            "setClipboard",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&text)],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

/// The text on Android's clipboard (`MainActivity.getClipboard`).
pub fn clipboard() -> Result<String, String> {
    with_activity(|env, activity| {
        let text = env
            .call_method(activity, "getClipboard", "()Ljava/lang/String;", &[])
            .and_then(|value| value.l())
            .map_err(|err| err.to_string())?;
        if text.is_null() {
            return Err("the clipboard is empty".into());
        }
        let text = JString::from(text);
        let text: String = env.get_string(&text).map_err(|err| err.to_string())?.into();
        Ok(text)
    })
}

/// A short tick of the vibration (`MainActivity.tick`), for a button of the touch
/// screen (`touch.rs`).
pub fn tick() {
    if let Err(err) = call_activity("tick", "()V", &[]) {
        bevy::log::warn_once!("the vibration: {err}");
    }
}

/// Calls a method of the game's activity (`MainActivity.java`) that takes no object.
fn call_activity(name: &str, signature: &str, args: &[JValue]) -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(activity, name, signature, args)
            .map_err(|err| err.to_string())?;
        Ok(())
    })
}

/// Runs `work` with the Java environment of this thread and the game's activity.
fn with_activity<T>(
    work: impl FnOnce(&mut jni::JNIEnv, &JObject) -> Result<T, String>,
) -> Result<T, String> {
    let app = bevy::android::ANDROID_APP
        .get()
        .ok_or("the app is not up yet")?;
    // SAFETY: the pointers are the process's Java VM and the activity's global
    // reference, which android-activity keeps alive while the app runs.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|err| err.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let mut env = vm.attach_current_thread().map_err(|err| err.to_string())?;
    let done = work(&mut env, &activity);
    // An exception of the call is said and cleared, so the thread can go on.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    done
}
