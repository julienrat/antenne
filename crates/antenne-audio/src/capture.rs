//! Capture d'un périphérique d'entrée via cpal (CoreAudio, WASAPI, PipeWire…).
//!
//! Le flux cpal est créé *dans* le thread de capture : `cpal::Stream` n'a pas
//! à être `Send` (il ne l'est pas sur toutes les plateformes).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::framer::Framer;
use crate::{FrameSink, SourceHandle, SourceStats};

/// Paramètres d'une capture.
#[derive(Debug, Clone)]
pub struct CaptureSpec {
    pub card_id: String,
    /// Noms candidats du périphérique (égalité stricte d'abord, puis
    /// « contient »), insensibles à la casse.
    pub device_patterns: Vec<String>,
    /// Premier canal du périphérique à lire.
    pub first_channel: u16,
    /// Nombre de canaux à extraire (1 ou 2).
    pub channels: u16,
    pub sample_rate: u32,
    /// Échantillons par canal et par trame.
    pub samples_per_frame: usize,
    /// Capacité du ring buffer, en échantillons (tous canaux).
    pub ring_capacity: usize,
}

/// Ouvre le périphérique et démarre la capture. Bloque au plus quelques
/// secondes, le temps que le flux soit ouvert (ou en échec).
pub fn spawn_capture(spec: CaptureSpec, mut sink: FrameSink) -> anyhow::Result<SourceHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(SourceStats::default());
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<anyhow::Result<String>>(1);

    let thread = {
        let stop = stop.clone();
        let stats = stats.clone();
        let spec = spec.clone();
        std::thread::Builder::new()
            .name(format!("capture-{}", spec.card_id))
            .spawn(move || {
                let (stream, mut consumer, device_name) = match open_stream(&spec, &stats) {
                    Ok(opened) => opened,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(device_name));

                // Thread « pompe » : vide le ring buffer et assemble les trames.
                let mut framer =
                    Framer::new(spec.sample_rate, spec.channels, spec.samples_per_frame);
                while !stop.load(Ordering::Relaxed) {
                    while let Ok(sample) = consumer.pop() {
                        if let Some(frame) = framer.push(sample) {
                            stats.frames.fetch_add(1, Ordering::Relaxed);
                            sink(frame);
                        }
                    }
                    // TODO(jalon 2) : remplacer l'attente active par un réveil
                    // calé sur la période du callback, et mesurer la latence.
                    std::thread::sleep(Duration::from_millis(1));
                }
                drop(stream);
            })?
    };

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(device_name)) => {
            tracing::info!(card = %spec.card_id, device = %device_name, "capture démarrée");
            Ok(SourceHandle::new(device_name, stop, stats, thread))
        }
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e.context(format!("carte « {} »", spec.card_id)))
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            bail!(
                "carte « {} » : délai dépassé à l'ouverture du périphérique",
                spec.card_id
            )
        }
    }
}

/// Cherche un périphérique d'entrée : égalité stricte, puis « contient ».
pub fn find_input_device(host: &cpal::Host, patterns: &[String]) -> anyhow::Result<cpal::Device> {
    let wanted: Vec<String> = patterns.iter().map(|p| p.to_lowercase()).collect();
    let mut devices: Vec<cpal::Device> = host
        .input_devices()
        .context("énumération des entrées audio")?
        .collect();
    let labels: Vec<String> = devices.iter().map(|d| d.to_string()).collect();
    let names: Vec<String> = labels.iter().map(|n| n.to_lowercase()).collect();

    let index = names
        .iter()
        .position(|n| wanted.iter().any(|w| n == w))
        .or_else(|| {
            names
                .iter()
                .position(|n| wanted.iter().any(|w| n.contains(w.as_str())))
        });
    match index {
        Some(i) => Ok(devices.swap_remove(i)),
        None => {
            let available = if labels.is_empty() {
                "aucune".to_string()
            } else {
                labels
                    .iter()
                    .take(12)
                    .map(|n| format!("« {n} »"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            bail!(
                "aucune entrée audio ne correspond à {patterns:?} ; entrées disponibles : {available}"
            )
        }
    }
}

fn open_stream(
    spec: &CaptureSpec,
    stats: &Arc<SourceStats>,
) -> anyhow::Result<(cpal::Stream, rtrb::Consumer<f32>, String)> {
    let host = cpal::default_host();

    // Une carte virtuelle fraîchement créée peut mettre un instant à apparaître.
    let mut attempt = 0;
    let device = loop {
        match find_input_device(&host, &spec.device_patterns) {
            Ok(device) => break device,
            Err(e) if attempt >= 10 => return Err(e),
            Err(_) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    };
    let device_name = device.to_string();

    let needed = spec.first_channel + spec.channels;
    let supported = pick_config(&device, spec.sample_rate, needed)
        .with_context(|| format!("périphérique « {device_name} »"))?;
    let device_channels = usize::from(supported.channels());
    let config: cpal::StreamConfig = supported.config();
    // TODO(jalon 2) : config.buffer_size = BufferSize::Fixed(..) pour réduire la latence.

    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(spec.ring_capacity);
    let first = usize::from(spec.first_channel);
    let count = usize::from(spec.channels);
    let route = Route {
        device_channels,
        first,
        count,
    };

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, config, producer, route, stats.clone())?,
        cpal::SampleFormat::I16 => build::<i16>(&device, config, producer, route, stats.clone())?,
        cpal::SampleFormat::I32 => build::<i32>(&device, config, producer, route, stats.clone())?,
        cpal::SampleFormat::U16 => build::<u16>(&device, config, producer, route, stats.clone())?,
        other => bail!("format d'échantillon non géré : {other:?}"),
    };
    stream.play().context("démarrage du flux")?;

    Ok((stream, consumer, device_name))
}

/// Canaux à extraire d'une trame du périphérique.
#[derive(Debug, Clone, Copy)]
struct Route {
    device_channels: usize,
    first: usize,
    count: usize,
}

/// Choisit une configuration d'entrée à `rate` Hz avec au moins `min_channels`.
fn pick_config(
    device: &cpal::Device,
    rate: u32,
    min_channels: u16,
) -> anyhow::Result<cpal::SupportedStreamConfig> {
    let default = device.default_input_config()?;
    if default.sample_rate() == rate && default.channels() >= min_channels {
        return Ok(default);
    }
    for range in device.supported_input_configs()? {
        if range.channels() < min_channels {
            continue;
        }
        if let Some(config) = range.try_with_sample_rate(rate) {
            return Ok(config);
        }
    }
    bail!(
        "aucune configuration à {rate} Hz avec {min_channels} canaux ou plus \
         (défaut du périphérique : {} Hz, {} canaux). Rééchantillonnage : TODO jalon 2",
        default.sample_rate(),
        default.channels()
    )
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    route: Route,
    stats: Arc<SourceStats>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let Route {
        device_channels,
        first,
        count,
    } = route;
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // ⚠ TEMPS RÉEL : ni allocation, ni verrou, ni log, ni appel système.
            for frame in data.chunks_exact(device_channels) {
                for &sample in &frame[first..first + count] {
                    if producer.push(sample.to_sample::<f32>()).is_err() {
                        stats.overruns.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        },
        |e: cpal::Error| tracing::warn!("flux audio : {e}"),
        None,
    )?;
    Ok(stream)
}
