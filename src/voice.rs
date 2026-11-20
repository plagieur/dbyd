//! Audio for voice channels: the microphone (sent to Discord) and the speakers (what others say).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use songbird::input::core::io::MediaSource;
use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Raw microphone audio as a byte stream of interleaved little-endian `f32`, which songbird encodes and sends.
/// When the microphone has nothing to give, it reads as silence so the audio thread never stalls.
pub struct MicSource {
    chunks: Mutex<Receiver<Vec<f32>>>,
    pending: Vec<u8>,
    frame_bytes: usize,
}

impl Read for MicSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            let received = self.chunks.lock().map_err(|_| io::Error::other("microphone lock"))?.recv_timeout(Duration::from_millis(20));
            match received {
                Ok(samples) => {
                    for s in samples {
                        self.pending.extend_from_slice(&s.to_le_bytes());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let n = (buf.len() / self.frame_bytes * self.frame_bytes).max(self.frame_bytes).min(buf.len());
                    buf[..n].fill(0);
                    return Ok(n);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Seek for MicSource {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

impl MediaSource for MicSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Keeps a cpal stream alive on its own thread (streams can't move between threads). Dropping it stops the stream.
pub struct Mic {
    _stop: mpsc::Sender<()>,
}

pub struct MicInfo {
    pub source: MicSource,
    pub sample_rate: u32,
    pub channels: u32,
}

/// Captures the microphone and mixes it down to mono (songbird only reads mono or stereo).
fn input_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig, channels: usize, tx: SyncSender<Vec<f32>>, muted: Arc<AtomicBool>) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                if muted.load(Ordering::Relaxed) {
                    return;
                }
                let mono = data.chunks(channels).map(|frame| frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / channels as f32).collect();
                let _ = tx.try_send(mono);
            },
            |err| eprintln!("microphone error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}

pub fn start_mic(muted: Arc<AtomicBool>) -> Result<(Mic, MicInfo), String> {
    let (info_tx, info_rx) = mpsc::channel::<Result<MicInfo, String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let setup = || -> Result<(cpal::Stream, MicInfo), String> {
            let device = cpal::default_host().default_input_device().ok_or("No microphone found.")?;
            let config = device.default_input_config().map_err(|e| e.to_string())?;
            let (sample_rate, device_channels) = (config.sample_rate(), config.channels() as usize);
            let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(64);
            let stream_config = config.config();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => input_stream::<f32>(&device, &stream_config, device_channels, tx, muted)?,
                cpal::SampleFormat::I16 => input_stream::<i16>(&device, &stream_config, device_channels, tx, muted)?,
                cpal::SampleFormat::U16 => input_stream::<u16>(&device, &stream_config, device_channels, tx, muted)?,
                other => return Err(format!("Unsupported microphone format: {other:?}")),
            };
            stream.play().map_err(|e| e.to_string())?;
            let source = MicSource { chunks: Mutex::new(rx), pending: Vec::new(), frame_bytes: 4 };
            Ok((stream, MicInfo { source, sample_rate, channels: 1 }))
        };
        match setup() {
            Ok((stream, info)) => {
                let _ = info_tx.send(Ok(info));
                let _ = stop_rx.recv(); // returns when the `Mic` is dropped
                drop(stream);
            }
            Err(e) => {
                let _ = info_tx.send(Err(e));
            }
        }
    });
    let info = info_rx.recv().map_err(|_| "The microphone thread stopped.".to_string())??;
    Ok((Mic { _stop: stop_tx }, info))
}

/// What others say, played on the default output device.
#[derive(Clone)]
pub struct Speaker {
    queue: Arc<Mutex<VecDeque<f32>>>,
    deafened: Arc<AtomicBool>,
    device_rate: u32,
    resampler: Arc<Mutex<Resampler>>,
}

/// Linear resampling that carries its position from one chunk to the next, so chunk edges don't click.
#[derive(Default)]
struct Resampler {
    previous: Option<[f32; 2]>,
    phase: f64,
}

pub struct SpeakerGuard {
    _stop: mpsc::Sender<()>,
}

impl Speaker {
    /// Adds 48 kHz interleaved stereo audio, converted to the device's sample rate.
    pub fn push(&self, pcm: &[i16]) {
        if self.deafened.load(Ordering::Relaxed) || pcm.len() < 2 {
            return;
        }
        let (Ok(mut queue), Ok(mut state)) = (self.queue.lock(), self.resampler.lock()) else { return };
        let frames = pcm.chunks_exact(2).map(|f| [f[0] as f32 / 32768.0, f[1] as f32 / 32768.0]);
        if self.device_rate == 48_000 {
            for frame in frames {
                queue.extend(frame);
            }
        } else {
            let step = 48_000.0 / self.device_rate as f64;
            let mut input: Vec<[f32; 2]> = state.previous.into_iter().collect();
            input.extend(frames);
            let last = input.len().saturating_sub(1);
            let mut phase = state.phase;
            while (phase as usize) < last {
                let a = phase as usize;
                let t = (phase - a as f64) as f32;
                for channel in 0..2 {
                    queue.push_back(input[a][channel] + (input[a + 1][channel] - input[a][channel]) * t);
                }
                phase += step;
            }
            state.previous = input.last().copied();
            state.phase = phase - last as f64;
        }
        let max = self.device_rate as usize / 3 * 2; // keep at most ~330 ms
        if queue.len() > max {
            let extra = queue.len() - max;
            queue.drain(..extra);
        }
    }
}

fn output_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig, channels: usize, device_rate: u32, queue: Arc<Mutex<VecDeque<f32>>>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    // start playing only once ~60 ms are queued, and wait again whenever the queue runs dry
    let prime = device_rate as usize * 2 * 60 / 1000;
    let mut primed = false;
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
                if !primed && q.len() >= prime {
                    primed = true;
                }
                for frame in data.chunks_mut(channels) {
                    let (l, r) = if primed {
                        match (q.pop_front(), q.pop_front()) {
                            (Some(l), Some(r)) => (l, r),
                            _ => {
                                primed = false;
                                (0.0, 0.0)
                            }
                        }
                    } else {
                        (0.0, 0.0)
                    };
                    for (i, out) in frame.iter_mut().enumerate() {
                        *out = T::from_sample(match (channels, i) {
                            (1, _) => (l + r) / 2.0,
                            (_, 0) => l,
                            (_, 1) => r,
                            _ => 0.0,
                        });
                    }
                }
            },
            |err| eprintln!("speaker error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}

pub fn start_speaker(deafened: Arc<AtomicBool>) -> Result<(SpeakerGuard, Speaker), String> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Speaker, String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let setup = || -> Result<(cpal::Stream, Speaker), String> {
            let device = cpal::default_host().default_output_device().ok_or("No audio output found.")?;
            let config = device.default_output_config().map_err(|e| e.to_string())?;
            let channels = config.channels() as usize;
            let device_rate = config.sample_rate();
            let queue = Arc::new(Mutex::new(VecDeque::new()));
            let stream_config = config.config();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => output_stream::<f32>(&device, &stream_config, channels, device_rate, queue.clone())?,
                cpal::SampleFormat::I16 => output_stream::<i16>(&device, &stream_config, channels, device_rate, queue.clone())?,
                cpal::SampleFormat::U16 => output_stream::<u16>(&device, &stream_config, channels, device_rate, queue.clone())?,
                other => return Err(format!("Unsupported audio format: {other:?}")),
            };
            stream.play().map_err(|e| e.to_string())?;
            Ok((stream, Speaker { queue, deafened, device_rate, resampler: Arc::new(Mutex::new(Resampler::default())) }))
        };
        match setup() {
            Ok((stream, speaker)) => {
                let _ = ready_tx.send(Ok(speaker));
                let _ = stop_rx.recv();
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        }
    });
    let speaker = ready_rx.recv().map_err(|_| "The audio thread stopped.".to_string())??;
    Ok((SpeakerGuard { _stop: stop_tx }, speaker))
}