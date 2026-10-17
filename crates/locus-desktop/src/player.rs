//! Native playback: one GStreamer `playbin` at a time. Video frames come out
//! of an appsink as RGBA and the picture shows the newest one; the position,
//! length and end of the file are read by [`Player::poll`]. The core decides
//! what plays. This only carries out its [`PlayerCmd`]s.

use std::sync::{Arc, Mutex};

use cosmic::widget::image::Handle;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::VideoFrameExt;
use locus_core::PlayerCmd;

/// The newest decoded frame, written by the appsink's thread.
#[derive(Default)]
struct Latest {
    fresh: bool,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// What [`Player::poll`] found since the last call.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub ended: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Player {
    pipeline: Option<gst::Element>,
    latest: Arc<Mutex<Latest>>,
}

/// Start a playbin for `uri`. The session `cookies` (`name=value`) go out with
/// the request, the way the gateway checks a signed-in client.
fn build(
    uri: &str,
    cookies: &[String],
    video: bool,
    latest: &Arc<Mutex<Latest>>,
) -> Result<gst::Element, String> {
    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|e| format!("GStreamer is not available: {e}"))?;
    playbin.set_property("uri", uri);
    if video {
        let sink = gst_app::AppSink::builder()
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .build(),
            )
            .max_buffers(1u32)
            .drop(true)
            .build();
        let latest = latest.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    store_frame(&sample, &latest);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        playbin.set_property("video-sink", &sink);
    }
    if !cookies.is_empty() {
        let cookies = cookies.to_vec();
        playbin.connect("source-setup", false, move |args| {
            if let Ok(source) = args[1].get::<gst::Element>()
                && source.has_property("cookies")
            {
                let refs: Vec<&str> = cookies.iter().map(String::as_str).collect();
                source.set_property("cookies", gst::glib::StrV::from(refs.as_slice()));
            }
            None
        });
    }
    Ok(playbin)
}

/// Copy the sample's picture into `latest` as tightly packed RGBA.
fn store_frame(sample: &gst::Sample, latest: &Arc<Mutex<Latest>>) {
    let (Some(caps), Some(buffer)) = (sample.caps(), sample.buffer()) else {
        return;
    };
    let Ok(info) = gst_video::VideoInfo::from_caps(caps) else {
        return;
    };
    let Ok(frame) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info) else {
        return;
    };
    let (w, h) = (info.width() as usize, info.height() as usize);
    let stride = frame.plane_stride()[0] as usize;
    let Ok(data) = frame.plane_data(0) else {
        return;
    };
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let start = row * stride;
        rgba.extend_from_slice(&data[start..start + w * 4]);
    }
    if let Ok(mut slot) = latest.lock() {
        *slot = Latest {
            fresh: true,
            width: w as u32,
            height: h as u32,
            rgba,
        };
    }
}

impl Player {
    /// Carry out one command. `uri` and `cookies` are only read by `Load`.
    pub fn command(
        &mut self,
        cmd: PlayerCmd,
        uri: Option<&str>,
        cookies: &[String],
    ) -> Result<(), String> {
        match cmd {
            PlayerCmd::Load {
                video, autoplay, ..
            } => {
                self.stop();
                let uri = uri.ok_or("no address for the file")?;
                let playbin = build(uri, cookies, video, &self.latest)?;
                let state = if autoplay {
                    gst::State::Playing
                } else {
                    gst::State::Paused
                };
                playbin
                    .set_state(state)
                    .map_err(|e| format!("could not start playback: {e}"))?;
                self.pipeline = Some(playbin);
                Ok(())
            }
            PlayerCmd::Pause => self.set_state(gst::State::Paused),
            PlayerCmd::Resume => self.set_state(gst::State::Playing),
            PlayerCmd::Seek(ms) => {
                if let Some(p) = &self.pipeline {
                    p.seek_simple(
                        gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                        gst::ClockTime::from_mseconds(ms),
                    )
                    .map_err(|e| format!("could not seek: {e}"))?;
                }
                Ok(())
            }
            PlayerCmd::Volume(v) => {
                if let Some(p) = &self.pipeline {
                    p.set_property("volume", f64::from(v.min(100)) / 100.0);
                }
                Ok(())
            }
            PlayerCmd::Stop => {
                self.stop();
                Ok(())
            }
        }
    }

    fn set_state(&self, state: gst::State) -> Result<(), String> {
        match &self.pipeline {
            Some(p) => p
                .set_state(state)
                .map(|_| ())
                .map_err(|e| format!("could not change playback: {e}")),
            None => Ok(()),
        }
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.pipeline.take() {
            let _ = p.set_state(gst::State::Null);
        }
        if let Ok(mut slot) = self.latest.lock() {
            *slot = Latest::default();
        }
    }

    /// The newest frame, once, when it is new since the last call.
    pub fn take_frame(&self) -> Option<Handle> {
        let mut slot = self.latest.lock().ok()?;
        if !slot.fresh {
            return None;
        }
        slot.fresh = false;
        Some(Handle::from_rgba(
            slot.width,
            slot.height,
            slot.rgba.clone(),
        ))
    }

    /// Read the position and length, and the end-of-stream and error messages.
    pub fn poll(&mut self) -> Report {
        let Some(p) = &self.pipeline else {
            return Report::default();
        };
        let mut report = Report {
            position_ms: p
                .query_position::<gst::ClockTime>()
                .map_or(0, |t| t.mseconds()),
            duration_ms: p
                .query_duration::<gst::ClockTime>()
                .map_or(0, |t| t.mseconds()),
            playing: p.current_state() == gst::State::Playing,
            ..Report::default()
        };
        if let Some(bus) = p.bus() {
            while let Some(msg) =
                bus.pop_filtered(&[gst::MessageType::Eos, gst::MessageType::Error])
            {
                match msg.view() {
                    gst::MessageView::Eos(_) => report.ended = true,
                    gst::MessageView::Error(e) => {
                        report.error = Some(e.error().to_string());
                    }
                    _ => {}
                }
            }
        }
        if report.ended {
            report.playing = false;
        }
        report
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
