var endpoint = 0;

export function get_content_web(path) {
	var xhttp = new XMLHttpRequest();
	xhttp.responseType = "arraybuffer";
	var response;
	//	xhttp.overrideMimeType('application/x-binary; charset=x-user-defined');
	xhttp.onload = function(oEvent) {
		response = xhttp.response;
	}

	var url = "content/" + path;
	xhttp.open("GET", url, true);
	xhttp.send(null);

	return response;
}

var total_requests = 0;
var completed_requests = 0;
var start = 0;

export function start_render()
{
	total_requests = completed_requests = 0;
	start = Math.floor(Date.now() / 1000);
}

export function http_request(x, y, tile_size, data) {
	total_requests++;
	document.getElementById('progress').innerText = "Rendering (" + completed_requests + " of " + total_requests + ")";
	var xhttp = new XMLHttpRequest();
	xhttp.onreadystatechange = function (oEvent) {
		if (xhttp.readyState === 4) {
			completed_requests++;
			if (total_requests == completed_requests) {
				var elapsed = Math.floor(Date.now() / 1000) - start;
				document.getElementById('progress').innerText = "Finished (" + completed_requests + " of " + total_requests + ") in " + elapsed + "secs";
			} else {
				document.getElementById('progress').innerText = "Rendering (" + completed_requests + " of " + total_requests + ")";
			}
			if (xhttp.status !== 200) {
				var canvas = document.getElementById("framebuffer");
				var ctx = canvas.getContext('2d');
				var dim = tile_size;
				var data = new Uint8ClampedArray(4 * dim * dim);
				var index=0;
				for (var i = 0; i < dim; ++i) {
					for (var j = 0; j < dim; ++j) {
						var base = i * dim + j;
						data[4 * base] = 255;
						data[4 * base + 1] = 0;
						data[4 * base + 2] = 0;
						data[4 * base + 3] = 255;
					}
				}
				var imageData = new ImageData(data, dim, dim);
				var tempcanvas = document.createElement('canvas');
				tempcanvas.width = dim;
				tempcanvas.height = dim;
				var tempctx = tempcanvas.getContext('2d');
				tempctx.putImageData(imageData, 0, 0);
				tempctx.textAlign = 'center';
				tempctx.fillText('error getting tile', dim/2, dim/2);
	
				//			ctx.scale(10,10);
				ctx.drawImage(tempcanvas, x*dim, y*dim);
			}
		}
	};
	xhttp.onload = function(oEvent) {
		var arraybuffer = xhttp.response;
		if (arraybuffer) {
			var canvas = document.getElementById("framebuffer");
			var ctx = canvas.getContext('2d');
			var byteArray = new Uint8Array(arraybuffer);

			var dim = tile_size;
			var data = new Uint8ClampedArray(4 * dim * dim);
			var index=0;
			for (var i = 0; i < dim; ++i) {
				for (var j = 0; j < dim; ++j) {
					var base = i * dim + j;
					data[4 * base] = byteArray[3*base];
					data[4 * base + 1] = byteArray[3*base+1];
					data[4 * base + 2] = byteArray[3*base+2];
					data[4 * base + 3] = 255;
				}
			}
			var imageData = new ImageData(data, dim, dim);
			var tempcanvas = document.createElement('canvas');
			tempcanvas.width = dim;
			tempcanvas.height = dim;
			var tempctx = tempcanvas.getContext('2d');
			tempctx.putImageData(imageData, 0, 0);

			//			ctx.scale(10,10);
			ctx.drawImage(tempcanvas, x*dim, y*dim);
		}
	};

//	var url = "https://pbrt-worker.edgecompute.app/rendertile";
	var url;
	if (endpoint == 0) {
		url = "https://pbrt-worker.edgecompute.app/rendertile";
		endpoint = 1;
	} else if (endpoint == 1) {
		url = "https://pbrt-worker2.edgecompute.app/rendertile";
		endpoint = 2;
	} else if (endpoint == 2) {
		url = "https://pbrt-worker3.edgecompute.app/rendertile";
		endpoint = 3;
	} else {
		url = "https://pbrt-worker4.edgecompute.app/rendertile";
		endpoint = 0;
	}
	xhttp.open("POST", url, true);
	xhttp.responseType = "arraybuffer";

	var body = {};
	body["x"] = x;
	body["y"] = y;
	body["tile_size"] = tile_size;
	body["filename"] = data;
	var body_str = JSON.stringify(body);

    xhttp.send(body_str);
}
