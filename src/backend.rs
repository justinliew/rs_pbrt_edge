use fastly::{Error,Request,Response,Body};
use image::error::DecodingError;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::str;

#[cfg(not(feature = "ecp"))]
#[cfg(not(test))]
use wasm_bindgen::prelude::*;

const PBRT_CONTENT_BACKEND_NAME: &str = "pbrt_content";

#[cfg(not(feature = "ecp"))]
#[cfg(not(test))]
#[wasm_bindgen(raw_module = "./request.js")]
extern "C" {
    pub fn get_content_web(data: String) -> Vec<u8>;
}

pub fn get_content_string(path: &str) -> Result<String, Error> {

	#[cfg(feature = "ecp")]
	{
		let url = format!("https://pbrt-edge.s3.us-west-2.amazonaws.com/{}", path);
		let mut b = Request::new("GET", url);
		b.set_ttl(60 * 10);
		println!("URL Path: {}", b.get_url_str());
		let mut resp = b.send(PBRT_CONTENT_BACKEND_NAME)?;
		let body = resp.take_body();
		Ok(body.into_string())
	}
	#[cfg(not(feature = "ecp"))]
	{
		let body : Vec<u8> = get_content_web(path.to_string());
		let msg = format!("Could not get string data from {}", path);
		let ret = str::from_utf8(&body).expect(&msg);
		Ok(ret.to_string())
	}
}

pub fn get_content_binary(path: &str) -> Result<Vec<u8>, Error> {

	#[cfg(feature = "ecp")]
	{
		let url = format!("https://pbrt-edge.s3.us-west-2.amazonaws.com{}", path);
		let mut b = Request::new("GET", url);
		b.set_ttl(60 * 10);
		println!("URL Path: {}", b.get_url_str());
		let mut resp = b.send(PBRT_CONTENT_BACKEND_NAME)?;
		let body = resp.take_body();
		Ok(body.into_bytes())
	}
	#[cfg(not(feature = "ecp"))]
	{
		// // JLTODO
		// let fullpath = format!("ganesha/{}", path);
		// let body : Vec<u8> = get_content_web(fullpath);
		// Ok(body)
		return Ok(vec![])
	}

}

