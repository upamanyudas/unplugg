use windows::Devices::Enumeration::{DeviceClass, DeviceInformation};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

unsafe fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
}

unsafe fn name(device: &IMMDevice) -> Option<String> {
    let store = device.OpenPropertyStore(STGM_READ).ok()?;
    let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    let string = PropVariantToStringAlloc(&value).ok();
    let result = string.and_then(|s| {
        let text = s.to_string().ok();
        CoTaskMemFree(Some(s.0.cast()));
        text
    });
    let _ = PropVariantClear(&mut value);
    result
}

unsafe fn audio_devices(
    enumerator: &IMMDeviceEnumerator,
    flow: EDataFlow,
) -> Vec<(String, IMMDevice)> {
    let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
        return Vec::new();
    };
    (0..collection.GetCount().unwrap_or(0))
        .filter_map(|index| {
            let device = collection.Item(index).ok()?;
            Some((name(&device)?, device))
        })
        .collect()
}

// The caller owns COM initialization for the lifetime of the returned endpoint.
pub unsafe fn endpoint(flow: EDataFlow, selected: &str) -> windows::core::Result<IMMDevice> {
    let enumerator = enumerator()?;
    if !selected.is_empty() {
        if let Some((_, device)) = audio_devices(&enumerator, flow)
            .into_iter()
            .find(|(name, _)| name == selected)
        {
            return Ok(device);
        }
    }
    enumerator.GetDefaultAudioEndpoint(flow, eConsole)
}

pub unsafe fn microphone(selected: &str) -> Option<IMMDevice> {
    endpoint(eCapture, selected).ok()
}

fn audio_names(flow: EDataFlow) -> Vec<String> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let names = enumerator()
            .map(|enumerator| audio_devices(&enumerator, flow))
            .unwrap_or_default()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        if initialized {
            CoUninitialize();
        }
        names
    }
}

pub fn microphones() -> Vec<String> {
    audio_names(eCapture)
}

pub fn speakers() -> Vec<String> {
    audio_names(eRender)
}

fn video_devices() -> Vec<(String, String, bool)> {
    std::thread::spawn(|| {
        if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
            return Vec::new();
        }
        let result = (|| {
            let operation =
                DeviceInformation::FindAllAsyncDeviceClass(DeviceClass::VideoCapture).ok()?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while operation.Status().ok()?.0 == 0 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if operation.Status().ok()?.0 != 1 {
                let _ = operation.Cancel();
                return None;
            }
            let devices = operation.GetResults().ok()?;
            Some(
                (0..devices.Size().ok()?)
                    .filter_map(|index| {
                        let device = devices.GetAt(index).ok()?;
                        Some((
                            device.Name().ok()?.to_string(),
                            device.Id().ok()?.to_string(),
                            device.IsDefault().unwrap_or(false),
                        ))
                    })
                    .collect(),
            )
        })()
        .unwrap_or_default();
        unsafe { RoUninitialize() };
        result
    })
    .join()
    .unwrap_or_default()
}

pub fn cameras() -> Vec<String> {
    video_devices()
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

pub fn camera(selected: &str) -> Option<String> {
    let devices = video_devices();
    devices
        .iter()
        .find(|(name, _, _)| !selected.is_empty() && name == selected)
        .or_else(|| devices.iter().find(|(_, _, default)| *default))
        .or_else(|| devices.first())
        .map(|(_, id, _)| id.clone())
}

pub fn default_microphone_name() -> Option<String> {
    unsafe {
        let enumerator = enumerator().ok()?;
        name(
            &enumerator
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .ok()?,
        )
    }
}

pub fn default_camera_name() -> Option<String> {
    let devices = video_devices();
    devices
        .iter()
        .find(|(_, _, default)| *default)
        .or_else(|| devices.first())
        .map(|(name, _, _)| name.clone())
}
