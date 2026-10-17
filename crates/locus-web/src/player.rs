//! Browser playback: one media element at a time. The core decides what plays
//! and this carries out its [`PlayerCmd`]s. The element is never drawn by the
//! browser: each frame of a video is copied out of it, and the page shows that
//! picture in iced, the same way the desktop app shows its frames.

use iced::widget::image;
use locus_core::PlayerCmd;
use wasm_bindgen::{Clamped, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, HtmlElement, HtmlMediaElement, HtmlVideoElement,
};

/// Widest a frame is copied at. A movie's picture is scaled down to this, so a
/// frame is a few megabytes at most.
const MAX_FRAME_W: u32 = 960;

/// What [`Player::poll`] found since the last call.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub ended: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Player {
    media: Option<HtmlMediaElement>,
    video: bool,
    /// The offscreen canvas frames are copied through, made on the first frame.
    canvas: Option<(HtmlCanvasElement, CanvasRenderingContext2d)>,
    /// The end and the error of the current file are reported once each.
    ended_sent: bool,
    error_sent: bool,
}

impl Player {
    /// Carry out one command. `uri` is only read by `Load`.
    pub fn command(&mut self, cmd: PlayerCmd, uri: Option<&str>) -> Result<(), String> {
        match cmd {
            PlayerCmd::Load {
                video, autoplay, ..
            } => {
                self.stop();
                let uri = uri.ok_or("no address for the file")?;
                let media = create(video)?;
                media.set_src(uri);
                self.media = Some(media);
                self.video = video;
                self.ended_sent = false;
                self.error_sent = false;
                if autoplay {
                    self.play();
                }
                Ok(())
            }
            PlayerCmd::Pause => {
                if let Some(m) = &self.media {
                    let _ = m.pause();
                }
                Ok(())
            }
            PlayerCmd::Resume => {
                self.play();
                Ok(())
            }
            PlayerCmd::Seek(ms) => {
                if let Some(m) = &self.media {
                    m.set_current_time(ms as f64 / 1000.0);
                }
                Ok(())
            }
            PlayerCmd::Volume(v) => {
                if let Some(m) = &self.media {
                    m.set_volume(f64::from(v.min(100)) / 100.0);
                }
                Ok(())
            }
            PlayerCmd::Stop => {
                self.stop();
                Ok(())
            }
        }
    }

    /// Start or resume. A browser may refuse (no user gesture); the element
    /// then stays paused and the transport shows Play.
    fn play(&self) {
        let Some(m) = &self.media else {
            return;
        };
        if let Ok(promise) = m.play() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = JsFuture::from(promise).await;
            });
        }
    }

    /// Whether the current file is a video (its picture needs a stage).
    pub fn is_video(&self) -> bool {
        self.media.is_some() && self.video
    }

    pub fn stop(&mut self) {
        if let Some(m) = self.media.take() {
            let _ = m.pause();
            m.remove_attribute("src").ok();
            m.load();
            m.remove();
        }
        self.video = false;
    }

    /// The picture as it is now, copied out of the video, or `None` before the
    /// first frame is known.
    pub fn frame(&mut self) -> Option<image::Handle> {
        if !self.is_video() {
            return None;
        }
        let video: HtmlVideoElement = self.media.as_ref()?.clone().dyn_into().ok()?;
        let (src_w, src_h) = (video.video_width(), video.video_height());
        if src_w == 0 || src_h == 0 {
            return None;
        }
        let w = src_w.min(MAX_FRAME_W);
        let h = (u64::from(src_h) * u64::from(w) / u64::from(src_w)).max(1) as u32;
        let ctx = self.context_of(w, h)?;
        ctx.draw_image_with_html_video_element_and_dw_and_dh(
            &video,
            0.0,
            0.0,
            f64::from(w),
            f64::from(h),
        )
        .ok()?;
        let pixels: Clamped<Vec<u8>> = ctx
            .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))
            .ok()?
            .data();
        Some(image::Handle::from_rgba(w, h, pixels.0))
    }

    /// The copy canvas's context, sized `w` by `h`. The canvas is made the
    /// first time and resized when the picture changes shape.
    fn context_of(&mut self, w: u32, h: u32) -> Option<CanvasRenderingContext2d> {
        if self.canvas.is_none() {
            let doc = web_sys::window()?.document()?;
            let canvas: HtmlCanvasElement = doc.create_element("canvas").ok()?.dyn_into().ok()?;
            let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
            self.canvas = Some((canvas, ctx));
        }
        let (canvas, ctx) = self.canvas.as_ref()?;
        if canvas.width() != w || canvas.height() != h {
            canvas.set_width(w);
            canvas.set_height(h);
        }
        Some(ctx.clone())
    }

    /// Read the position and length, and the end and error once each.
    pub fn poll(&mut self) -> Report {
        let Some(m) = &self.media else {
            return Report::default();
        };
        let duration = m.duration();
        let mut report = Report {
            position_ms: secs_to_ms(m.current_time()),
            duration_ms: if duration.is_finite() {
                secs_to_ms(duration)
            } else {
                0
            },
            playing: !m.paused() && !m.ended(),
            ..Report::default()
        };
        if !m.ended() {
            // Seeking back or playing again makes a later end news.
            self.ended_sent = false;
        } else if !self.ended_sent {
            self.ended_sent = true;
            report.ended = true;
        }
        if let Some(error) = m.error()
            && !self.error_sent
        {
            self.error_sent = true;
            report.error = Some(media_error_text(error.code()).to_string());
        }
        report
    }
}

/// A media element for audio or video. It stays off the visible page: video
/// frames are copied out of it, and audio needs no picture.
fn create(video: bool) -> Result<HtmlMediaElement, String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let doc = window.document().ok_or("no page")?;
    let tag = if video { "video" } else { "audio" };
    let el: HtmlMediaElement = doc
        .create_element(tag)
        .map_err(|_| format!("could not create the {tag} player"))?
        .dyn_into()
        .map_err(|_| format!("could not create the {tag} player"))?;
    if video {
        // A video only decodes frames while it is on the page, so it joins it
        // hidden.
        let html: &HtmlElement = el.unchecked_ref();
        let _ = html.style().set_property("display", "none");
        let body = doc.body().ok_or("no page body")?;
        body.append_child(&el)
            .map_err(|_| "could not start the video".to_string())?;
    }
    Ok(el)
}

fn secs_to_ms(secs: f64) -> u64 {
    if secs.is_finite() && secs > 0.0 {
        (secs * 1000.0) as u64
    } else {
        0
    }
}

/// The browser's media error codes, in words.
fn media_error_text(code: u16) -> &'static str {
    match code {
        1 => "playback was stopped",
        2 => "the file could not be downloaded",
        3 => "the file could not be decoded",
        4 => "this browser cannot play the file",
        _ => "playback failed",
    }
}
