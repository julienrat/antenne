//! Inventaire des entrées audio visibles par cpal, tous hôtes confondus.

use cpal::traits::{DeviceTrait, HostTrait};

/// Description d'une entrée audio.
#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    /// Hôte cpal (Alsa, PipeWire, CoreAudio, Wasapi…).
    pub host: String,
    pub name: String,
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
    pub sample_format: Option<String>,
}

/// Liste les entrées de tous les hôtes disponibles.
pub fn list_input_devices() -> Vec<InputDeviceInfo> {
    let mut out = Vec::new();
    for host_id in cpal::available_hosts() {
        let host = match cpal::host_from_id(host_id) {
            Ok(host) => host,
            Err(e) => {
                tracing::warn!("hôte {host_id:?} indisponible : {e}");
                continue;
            }
        };
        let devices = match host.input_devices() {
            Ok(devices) => devices,
            Err(e) => {
                tracing::warn!("hôte {host_id:?} : énumération impossible : {e}");
                continue;
            }
        };
        for device in devices {
            let config = device.default_input_config().ok();
            out.push(InputDeviceInfo {
                host: format!("{host_id:?}"),
                name: device.to_string(),
                channels: config.as_ref().map(|c| c.channels()),
                sample_rate: config.as_ref().map(|c| c.sample_rate()),
                sample_format: config.as_ref().map(|c| format!("{:?}", c.sample_format())),
            });
        }
    }
    out
}

/// Noms des entrées de l'hôte par défaut : celui qu'utilise la capture, donc
/// les seuls noms valables pour `source.device`.
pub fn default_host_input_names() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.map(|d| d.to_string()).collect(),
        Err(e) => {
            tracing::warn!("énumération des entrées audio impossible : {e}");
            Vec::new()
        }
    }
}
