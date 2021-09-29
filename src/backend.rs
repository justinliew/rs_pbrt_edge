use fastly::{Error,Request,Response,Body};

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
		let body = get_content_web(path);
		Ok(body)
	}
}

pub fn get_content_binary(path: &str) -> Result<Vec<u8>, Error> {

	#[cfg(feature = "ecp")]
	{
		let url = format!("https://pbrt-edge.s3.us-west-2.amazonaws.com/{}", path);
		let mut b = Request::new("GET", url);
		b.set_ttl(60 * 10);
		println!("URL Path: {}", b.get_url_str());
		let mut resp = b.send(PBRT_CONTENT_BACKEND_NAME)?;
		let body = resp.take_body();
		Ok(body.into_bytes())
	}
	#[cfg(not(feature = "ecp"))]
	{
		let body = get_content_web(path);
		Ok(body)
	}
}

