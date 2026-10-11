//! `GET /thumb/{sha}?size=N`: cached JPEG thumbnails of image blobs.

use std::io;
use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ImageEncoder, ImageFormat, ImageReader, Limits};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::blob::{blob_path, is_regular, unauthorized, valid_sha, with_cache_headers};
use crate::{SharedState, auth, unique_name};

const DEFAULT_SIZE: u32 = 256;
const MIN_SIZE: u32 = 64;
const MAX_SIZE: u32 = 1024;
const JPEG_QUALITY: u8 = 85;
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
pub struct ThumbQuery {
    size: Option<u32>,
}

enum RenderError {
    Unsupported,
    Io(io::Error),
}

pub(crate) async fn thumbnail(
    State(st): State<SharedState>,
    headers: HeaderMap,
    AxPath(sha): AxPath<String>,
    Query(q): Query<ThumbQuery>,
) -> Response {
    if !auth::authorized(&st, &headers).await {
        return unauthorized();
    }
    if !valid_sha(&sha) {
        return (StatusCode::BAD_REQUEST, "invalid sha256").into_response();
    }
    let size = q.size.unwrap_or(DEFAULT_SIZE).clamp(MIN_SIZE, MAX_SIZE);
    let data_dir = st.config.data_dir.clone();
    let dir = data_dir.join("thumbs").join(size.to_string());
    let dest = dir.join(format!("{sha}.jpg"));

    if !is_regular(&dest).await {
        let src = blob_path(&data_dir, &sha);
        if !is_regular(&src).await {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
        let tmp = data_dir.join("tmp").join(unique_name("thumb"));
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            tracing::error!(error = %e, "thumb dir create failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        let (src2, tmp2) = (src.clone(), tmp.clone());
        let rendered = tokio::task::spawn_blocking(move || render(&src2, &tmp2, size)).await;
        let outcome = match rendered {
            Ok(Ok(())) => tokio::fs::rename(&tmp, &dest)
                .await
                .map_err(RenderError::Io),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(RenderError::Io(io::Error::other(e))),
        };
        if let Err(e) = outcome {
            let _ = tokio::fs::remove_file(&tmp).await;
            return match e {
                RenderError::Unsupported => {
                    (StatusCode::UNSUPPORTED_MEDIA_TYPE, "not an image").into_response()
                }
                RenderError::Io(e) => {
                    tracing::error!(error = %e, "thumbnail render failed");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            };
        }
    }

    serve_thumb(&dest, &sha, size).await
}

async fn serve_thumb(path: &Path, sha: &str, size: u32) -> Response {
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let len = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CONTENT_LENGTH, len)
        .header(header::ETAG, format!("\"thumb-{size}-{sha}\""));
    with_cache_headers(builder)
        .body(Body::from_stream(ReaderStream::new(file)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Decode `src`, fit it within `size` x `size`, and write a JPEG to `tmp`.
fn render(src: &Path, tmp: &Path, size: u32) -> Result<(), RenderError> {
    let mut reader = ImageReader::open(src)
        .and_then(|r| r.with_guessed_format())
        .map_err(RenderError::Io)?;
    match reader.format() {
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP) => {}
        _ => return Err(RenderError::Unsupported),
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let img = reader.decode().map_err(|_| RenderError::Unsupported)?;

    let rgb = img.resize(size, size, FilterType::Lanczos3).to_rgb8();
    let file = std::fs::File::create(tmp).map_err(RenderError::Io)?;
    let mut writer = io::BufWriter::new(file);
    JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| RenderError::Io(io::Error::other(e)))?;
    let file = writer
        .into_inner()
        .map_err(|e| RenderError::Io(e.into_error()))?;
    file.sync_all().map_err(RenderError::Io)?;
    Ok(())
}
