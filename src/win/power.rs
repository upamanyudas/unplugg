use std::ptr::null_mut;

use serde_json::{Map, Value};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::System::Power::{
    PowerGetActiveScheme, PowerReadACValueIndex, PowerReadDCValueIndex, PowerSetActiveScheme,
    PowerWriteACValueIndex, PowerWriteDCValueIndex, SetThreadExecutionState, ES_CONTINUOUS,
    ES_SYSTEM_REQUIRED,
};

const BUTTONS: GUID = GUID::from_u128(0x4f971e89_eebd_4455_a8de_9e59040e7347);
const ACTIONS: [(&str, GUID); 3] = [
    (
        "5ca83367-6e45-459f-a27b-476b1d01c936",
        GUID::from_u128(0x5ca83367_6e45_459f_a27b_476b1d01c936),
    ),
    (
        "7648efa3-dd9c-4e3e-b566-50f929386280",
        GUID::from_u128(0x7648efa3_dd9c_4e3e_b566_50f929386280),
    ),
    (
        "96996bc0-ad50-47ec-923b-6f41874dd9eb",
        GUID::from_u128(0x96996bc0_ad50_47ec_923b_6f41874dd9eb),
    ),
];

fn scheme() -> Option<GUID> {
    unsafe {
        let mut ptr = null_mut();
        if PowerGetActiveScheme(null_mut(), &mut ptr) != 0 || ptr.is_null() {
            return None;
        }
        let guid = *ptr;
        LocalFree(ptr.cast());
        Some(guid)
    }
}

fn read(scheme: &GUID, setting: &GUID, ac: bool) -> Option<u32> {
    let mut value = 0;
    let result = unsafe {
        if ac {
            PowerReadACValueIndex(null_mut(), scheme, &BUTTONS, setting, &mut value)
        } else {
            PowerReadDCValueIndex(null_mut(), scheme, &BUTTONS, setting, &mut value)
        }
    };
    (result == 0).then_some(value)
}

fn write(scheme: &GUID, setting: &GUID, ac: bool, value: u32) -> bool {
    unsafe {
        (if ac {
            PowerWriteACValueIndex(null_mut(), scheme, &BUTTONS, setting, value)
        } else {
            PowerWriteDCValueIndex(null_mut(), scheme, &BUTTONS, setting, value)
        }) == 0
    }
}

pub(super) fn hold(config: &mut Map<String, Value>) -> bool {
    unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
    let Some(scheme) = scheme() else { return false };
    let mut restore = Map::new();
    for (name, setting) in &ACTIONS {
        for ac in [true, false] {
            let Some(value) = read(&scheme, setting, ac) else {
                return false;
            };
            restore.insert(format!("{name}/{}", u8::from(ac)), value.into());
        }
    }
    config.insert("power_restore".into(), restore.into());
    if crate::config::save_config(config).is_err() {
        return false;
    }
    let mut success = true;
    for (_, setting) in &ACTIONS {
        for ac in [true, false] {
            success &= write(&scheme, setting, ac, 0);
        }
    }
    success & (unsafe { PowerSetActiveScheme(null_mut(), &scheme) } == 0)
}

pub(super) fn release(config: &mut Map<String, Value>) -> bool {
    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    let Some(restore) = config.get("power_restore").and_then(Value::as_object) else {
        return true;
    };
    if restore.is_empty() {
        return true;
    }
    let Some(scheme) = scheme() else { return false };
    let mut success = true;
    for (name, setting) in &ACTIONS {
        for ac in [true, false] {
            let key = format!("{name}/{}", u8::from(ac));
            if let Some(value) = restore
                .get(&key)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
            {
                success &= write(&scheme, setting, ac, value);
            }
        }
    }
    success &= unsafe { PowerSetActiveScheme(null_mut(), &scheme) } == 0;
    if success {
        config.insert("power_restore".into(), Value::Object(Map::new()));
        success = crate::config::save_config(config).is_ok();
    }
    success
}
