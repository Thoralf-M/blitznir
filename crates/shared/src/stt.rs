//! Parakeet Ultra (moondream's post-train of Parakeet TDT 0.6B v3; ONNX, CPU) behind core's
//! `Recognizer`, and its download.

use blitznir_core::{
    Error, Result,
    config::{Paths, SttModel},
    platform::{Recognizer, Token},
    segmenter::SAMPLE_RATE,
};
use parakeet_rs::{ExecutionConfig, ParakeetTDT, TimestampMode, Transcriber};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// istupakov's v3 graphs with Ultra's weights: same files, tensor names and IO as the v3 export.
// Better German (FLEURS 4.13 → 3.61 % WER), English and noise than v3, by moondream's card.
const REPO: &str = "Masterx/parakeet-tdt-0.6b-ultra-onnx";
const REVISION: &str = "99b09f030a5a6efeaa13cf2cf54592100ce2c3f1";

/// Files and byte sizes at REVISION, as the HF API lists them. One
/// variant per directory: the loader silently prefers fp32 when both are present.
fn files(m: SttModel) -> &'static [(&'static str, u64)] {
    match m {
        SttModel::Fp32 => &[
            ("encoder-model.onnx", 41_770_866),
            ("encoder-model.onnx.data", 2_435_420_160),
            ("decoder_joint-model.onnx", 72_520_893),
            ("vocab.txt", 93_939),
        ],
        SttModel::Int8 => &[
            ("encoder-model.int8.onnx", 652_183_214),
            ("decoder_joint-model.int8.onnx", 18_202_004),
            ("vocab.txt", 93_939),
        ],
    }
}

/// Its own directory: the fp32 files have the v3 files' exact sizes, and presence is checked by size.
pub fn model_dir(paths: &Paths, m: SttModel) -> PathBuf {
    paths.models().join("parakeet-ultra").join(m.as_str())
}

/// The files of `m` that are missing from `dir` or have the wrong size.
fn missing(dir: &Path, m: SttModel) -> Vec<(&'static str, u64)> {
    files(m)
        .iter()
        .copied()
        .filter(|(name, size)| !std::fs::metadata(dir.join(name)).is_ok_and(|md| md.len() == *size))
        .collect()
}

pub fn model_present(dir: &Path, m: SttModel) -> bool {
    missing(dir, m).is_empty()
}

/// Download every file that is missing or has the wrong size; `progress`
/// hears the percentage of the whole download each time it grows.
pub fn ensure_model(dir: &Path, m: SttModel, progress: &mut dyn FnMut(u64)) -> Result<()> {
    let todo = missing(dir, m);
    if todo.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let total = todo.iter().map(|(_, size)| size).sum::<u64>().max(1);
    let (mut done, mut shown) = (0, 0);
    for (name, size) in todo {
        let url = format!("https://huggingface.co/{REPO}/resolve/{REVISION}/{name}");
        download(&url, &dir.join(name), size, &mut |n| {
            done += n;
            if done * 100 / total > shown {
                shown = done * 100 / total;
                progress(shown);
            }
        })?;
    }
    Ok(())
}

/// To `dest.part`, then renamed once the size is right: a broken download
/// never looks like a model. `got_bytes` hears every block's byte count.
pub(crate) fn download(
    url: &str,
    dest: &Path,
    size: u64,
    got_bytes: &mut dyn FnMut(u64),
) -> Result<()> {
    log::info!("downloading {url} ({} MB)", size / 1_000_000);
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(None::<Duration>)
        .build()?;
    let mut resp = client.get(url).send()?.error_for_status()?;
    let mut file = std::fs::File::create(&part)?;
    let (mut buf, mut got) = (vec![0; 1 << 20], 0_u64);
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        got += n as u64;
        got_bytes(n as u64);
    }
    file.sync_all()?;
    if got != size {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Stt(format!(
            "{url}: got {got} bytes, expected {size}"
        )));
    }
    Ok(std::fs::rename(&part, dest)?)
}

fn stt_error(e: parakeet_rs::Error) -> Error {
    Error::Stt(e.to_string())
}

pub struct Parakeet(ParakeetTDT);

impl Parakeet {
    pub fn load(dir: &Path) -> Result<Parakeet> {
        let started = Instant::now();
        // past 4 intra-op threads the encoder barely scales (measured)
        let cfg = ExecutionConfig::new().with_intra_threads(4);
        let mut p = Parakeet(ParakeetTDT::from_pretrained(dir, Some(cfg)).map_err(stt_error)?);
        // the first decode pays one-off setup costs; not the user's first words
        p.transcribe(&vec![0.0; SAMPLE_RATE / 2])?;
        log::info!(
            "loaded {} in {:.1} s",
            dir.display(),
            started.elapsed().as_secs_f32()
        );
        Ok(p)
    }
}

impl Recognizer for Parakeet {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<Vec<Token>> {
        // Tokens, not Words: Words silently drops repeated words
        let r = self.0.transcribe_samples(
            pcm16k.to_vec(),
            SAMPLE_RATE as u32,
            1,
            Some(TimestampMode::Tokens),
        );
        Ok(r.map_err(stt_error)?
            .tokens
            .into_iter()
            .map(|t| Token {
                text: t.text,
                end_s: t.end,
            })
            .collect())
    }
}
