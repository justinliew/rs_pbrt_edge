use fastly::http::{header, Method, StatusCode};
use fastly::{mime, Error, Request, Response};

#[cfg(feature = "ecp")]
#[fastly::main]
fn main(mut req: Request) -> Result<Response, Error> {
    // Filter request methods...
    match req.get_method() {
        // Allow GET and HEAD requests.
        &Method::GET | &Method::HEAD => (),

        // Deny anything else.
        _ => {
            return Ok(Response::from_status(StatusCode::METHOD_NOT_ALLOWED)
                .with_header(header::ALLOW, "GET, HEAD")
                .with_body_str("This method is not allowed\n"))
        }
    };

    // Pattern match on the path.
    match req.get_path() {

		// "/rendertile" => {
		// 	TODO call entry with specific params
		// 	let b = req.into_body();
		// 	let s = b.into_string();
		// 	let input : HittableListWithTile = serde_json::from_str(&s).unwrap();
		// 	let res = render::render_tile(&input.h, input.i,input.j, input.dimi, input.dimj, input.width, input.height);
		// 	let res_json = serde_json::to_string(&res).unwrap();
		// 	Ok(Response::from_status(StatusCode::OK)
		// 		.with_body(res_json))
		// 		// .with_content_type(mime::IMAGE_JPEG)
		// 		// .with_body(d))
		// }
        // If request is to the `/` path, send a default response.
        "/" => Ok(Response::from_status(StatusCode::OK)
            .with_content_type(mime::TEXT_HTML_UTF_8)
            .with_body("<iframe src='https://developer.fastly.com/compute-welcome' style='border:0; position: absolute; top: 0; left: 0; width: 100%; height: 100%'></iframe>\n")),

        // Catch all other requests and return a 404.
        _ => Ok(Response::from_status(StatusCode::NOT_FOUND)
            .with_body_str("The page you requested could not be found\n")),
    }
}
