//! JNI exports called by `app.dun.android.DunNative`.
//!
//! These must be defined in the `dun_lib` cdylib itself: exports from
//! dependency rlibs can be dropped by the linker, and loading a second library
//! with its own SQLite would corrupt the database.

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use crate::mobile::bridge;

#[no_mangle]
pub extern "system" fn Java_app_dun_android_DunNative_handleEvent<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
    tz_id: JString<'local>,
    now_ms: jlong,
    event_json: JString<'local>,
) -> jstring {
    let response = match (
        read(&mut env, &data_dir),
        read(&mut env, &tz_id),
        read(&mut env, &event_json),
    ) {
        (Ok(dir), Ok(tz), Ok(event)) => bridge::handle_event_json(&dir, &tz, now_ms, &event),
        (dir, tz, event) => {
            let err = [dir.err(), tz.err(), event.err()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("; ");
            serde_json::json!({ "ok": false, "error": format!("jni args: {err}"), "handlerMs": 0 })
                .to_string()
        }
    };

    match env.new_string(response) {
        Ok(s) => s.into_raw(),
        // An exception is already pending in the JVM; Kotlin's try/catch turns
        // it into a fail-loud notification.
        Err(_) => std::ptr::null_mut(),
    }
}

fn read(env: &mut JNIEnv, s: &JString) -> Result<String, String> {
    env.get_string(s).map(Into::into).map_err(|e| e.to_string())
}
