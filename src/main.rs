use fastly::http::header::HeaderValue;
use fastly::http::{header, Method, StatusCode};
use fastly::{mime, Error, Request, Response};

use std::time::Instant;

#[macro_use]
extern crate serde;

use serde::{Deserialize, Serialize};

#[macro_use]
extern crate impl_ops;

pub mod accelerators;
pub mod blockqueue;
pub mod cameras;
pub mod core;
mod entry;
pub mod filters;
pub mod integrators;
pub mod lights;
pub mod materials;
pub mod media;
pub mod samplers;
pub mod shapes;
pub mod textures;

#[derive(Clone, Serialize, Deserialize)]
pub struct RenderTileInfo {
    pub x: u32,
    pub y: u32,
    pub tile_size: i32,
    pub data: String,
    // pub dimi: usize,
    // pub dimj: usize,
    // pub height: usize,
    // pub width: usize,
}
//#[cfg(feature = "ecp")]
#[fastly::main]
fn main(mut req: Request) -> Result<Response, Error> {
    // Filter request methods...
    match req.get_method() {
        // Allow GET and HEAD requests.
        &Method::GET | &Method::HEAD | &Method::POST => (),

        // Deny anything else.
        _ => {
            return Ok(Response::from_status(StatusCode::METHOD_NOT_ALLOWED)
                .with_header(header::ALLOW, "GET, HEAD, POST")
                .with_header("Access-Control-Allow-Origin", HeaderValue::from_static("*"))
                .with_header("Vary", HeaderValue::from_static("Origin"))
                .with_body_str("This method is not allowed\n"))
        }
    };

    // Pattern match on the path.
    match req.get_path() {

		"/rendertile" => {
			let now = Instant::now();
			let b = req.into_body();
			let s = b.into_string();
			let input : RenderTileInfo = serde_json::from_str(&s).unwrap();
			let output = entry::entry(false, input.tile_size, Some(input.x), Some(input.y), &input.data);
			println!("Elapsed: {}", now.elapsed().as_millis());
			Ok(Response::from_status(StatusCode::OK)
				.with_header("Access-Control-Allow-Origin", HeaderValue::from_static("*"))
				.with_header("Vary", HeaderValue::from_static("Origin"))
				.with_body(output)
				.with_content_type(mime::APPLICATION_OCTET_STREAM))
				// .with_content_type(mime::IMAGE_JPEG)
				// .with_body(d))
		}
        // If request is to the `/` path, send a default response.
        "/" => Ok(Response::from_status(StatusCode::OK)
            .with_content_type(mime::TEXT_HTML_UTF_8)
            .with_body("<iframe src='https://developer.fastly.com/compute-welcome' style='border:0; position: absolute; top: 0; left: 0; width: 100%; height: 100%'></iframe>\n")),

        // Catch all other requests and return a 404.
        _ => Ok(Response::from_status(StatusCode::NOT_FOUND)
            .with_body_str("The page you requested could not be found\n")),
    }
}
